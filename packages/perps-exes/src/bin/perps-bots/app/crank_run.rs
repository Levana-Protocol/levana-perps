//! The crank and price systems are intricately tied together. Here's a basic overview of the theory of how they should operate together:
//!
//! Create three different subcomponents: price, crank watch, and crank update
//! 1. Create a signaling mechanism (some kind of a channel) from price and crank watch to crank update
//! 2. Price will be responsible for getting latest prices, checking if oracles need to be updated (and then performing those updates), and sending a signal to crank update if a price update should have triggered a liquidation
//! 3. Crank watch will not send any transactions, it will simply observe if there's crank work and send messages to crank update
//! 4. Both price and crank watch should be fully parallelized across markets. Price will get all the prices from Pyth at once, check all the markets in parallel, and then put together a single transaction for oracle updates
//! 5. Crank watch is much more simply fully parallelizable
//! 6. Crank update will watch its channel for work items and immediately jump into sending a transaction to up to X markets at once (I'm thinking 3 due to gas concerns)
//! 7. The goal here is to get info as quickly as possible that work needs to be done, as opposed to needing to loop through all the markets. I think a big contributing factor last week is the sheer number of markets we have on Osmosis now, it takes a while to process them serially

mod trigger_crank;

use std::borrow::Cow;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::async_trait;
use chrono::Utc;
use cosmos::proto::cosmos::base::abci::v1beta1::TxResponse;
use cosmos::{Address, HasAddress, TxBuilder, Wallet};
use msg::contracts::market::spot_price::{
    PythPriceServiceNetwork, SpotPriceConfig, SpotPriceFeedData,
};
use msg::prelude::MarketExecuteMsg;
use perps_exes::prelude::MarketContract;
use perps_exes::pyth::get_oracle_update_msg;
use shared::storage::RawAddr;

use crate::app::GasUsage;
use crate::util::oracle::OffchainPriceData;
use crate::watcher::{Heartbeat, WatchedTask, WatchedTaskOutput};

use self::trigger_crank::CrankReceiver;

use super::gas_check::GasCheckWallet;
use super::{App, AppBuilder};
pub(crate) use trigger_crank::TriggerCrank;

struct Worker {
    crank_wallet: Wallet,
    recv: CrankReceiver,
}

/// Start the background thread to turn the crank on the crank bots.
impl AppBuilder {
    pub(super) fn start_crank_run(&mut self) -> Result<Option<TriggerCrank>> {
        if self.app.config.crank_wallets.is_empty() {
            return Ok(None);
        }

        let recv = CrankReceiver::new();

        let crank_wallets = self.app.config.crank_wallets.clone();

        for (idx, crank_wallet) in crank_wallets.into_iter().enumerate() {
            self.refill_gas(crank_wallet.get_address(), GasCheckWallet::Crank(idx + 1))?;

            let worker = Worker {
                crank_wallet,
                recv: recv.clone(),
            };
            self.watch_periodic(
                crate::watcher::TaskLabel::CrankRun { index: idx + 1 },
                worker,
            )?;
        }

        Ok(Some(recv.trigger))
    }
}

#[async_trait]
impl WatchedTask for Worker {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        app.crank(&self.crank_wallet, &self.recv).await
    }
}

const CRANK_EXECS: &[u32] = &[7, 4, 1];

impl App {
    async fn crank(
        &self,
        crank_wallet: &Wallet,
        recv: &CrankReceiver,
    ) -> Result<WatchedTaskOutput> {
        // Wait for up to 20 seconds for new work to appear. If it doesn't, update our status message that no cranking was needed.
        let (market, market_id, crank_guard) = match recv.receive_with_timeout().await {
            None => {
                return Ok(WatchedTaskOutput::new("No crank work needed").suppress());
            }
            Some(crank_needed) => crank_needed,
        };

        let rewards = self
            .config
            .get_crank_rewards_wallet()
            .map(|a| a.get_address_string().into());

        let mut actual_execs = None;

        // Simulate decreasing numbers of execs until we find one that looks like it will pass.
        for execs in CRANK_EXECS {
            match TxBuilder::default()
                .add_execute_message(
                    market,
                    crank_wallet,
                    vec![],
                    MarketExecuteMsg::Crank {
                        execs: Some(*execs),
                        rewards: rewards.clone(),
                    },
                )?
                .simulate(&self.cosmos, &[crank_wallet.get_address()])
                .await
            {
                Ok(_) => {
                    actual_execs = Some(*execs);
                    break;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to simulate crank against market {market} with execs {execs}: {e}"
                    )
                }
            }
        }

        // Now that we've determined how many execs we think will work, now
        // submit the actual transaction. We separate out in this way to avoid
        // confusion about whether this fails during simulation or broadcasting,
        // so during Osmosis epochs we can safely ignore just the broadcasting.
        let mut builder = TxBuilder::default();

        builder.add_execute_message(
            market,
            crank_wallet,
            vec![],
            MarketExecuteMsg::Crank {
                execs: actual_execs,
                rewards: rewards.clone(),
            },
        )?;

        enum RunResult {
            NormalRun(TxResponse),
            RunWithOracle(TxResponse),
            OutOfGas,
        }

        let run_result = match builder
            .sign_and_broadcast(&self.cosmos, crank_wallet)
            .await
            .with_context(|| format!("Unable to turn crank for market {market}"))
        {
            Ok(txres) => {
                let mut gas_used = self.gas_usage.write().await;
                gas_used
                    .entry(crank_wallet.get_address())
                    .or_insert_with(|| GasUsage {
                        total: Default::default(),
                        entries: Default::default(),
                        usage_per_hour: Default::default(),
                    })
                    .add_entry(Utc::now(), txres.gas_used);
                RunResult::NormalRun(txres)
            }
            Err(e) => {
                if self.is_osmosis_epoch() {
                    return Ok(WatchedTaskOutput::new(format!("Ignoring crank run error since we think we're in the Osmosis epoch, error: {e:?}")));
                }

                let error_as_str = format!("{e:?}");

                // If we got an "out of gas" code 11 error, we want to ignore
                // it. This usually happens when new work comes in. The logic
                // below to check if new work is available will cause a new
                // crank run to be scheduled, if one is needed.
                if error_as_str.contains("out of gas") || error_as_str.contains("code 11") {
                    RunResult::OutOfGas
                }
                // Check if we hit price_too_old and, if so, try again with a transaction that includes an oracle update.
                else if error_as_str.contains("price_too_old") {
                    // We ignore price too old if Pyth updates are closed
                    if self.pyth_prices_closed(market, None).await? {
                        return Ok(WatchedTaskOutput::new(format!("Ignoring failed crank for {market_id} due to price_too_old since Pyth prices are currently closed")));
                    }
                    match self
                        .try_crank_with_oracle(market, crank_wallet, rewards)
                        .await
                        .with_context(|| {
                            format!("Unable to update oracle and turn crank for market {market_id} ({market})")
                        }) {
                        Ok(txres) => RunResult::RunWithOracle(txres),
                        Err(e2) => {
                            log::error!(
                                "Got price_too_old and cranking with oracle failed too: {e2:?}"
                            );
                            return Err(e2);
                        }
                    }
                } else {
                    return Err(e);
                }
            }
        };

        // Successfully cranked, check if there's more work and, if so, schedule it to be started again
        std::mem::drop(crank_guard);
        let more_work = match MarketContract::new(self.cosmos.make_contract(market))
            .status()
            .await
        {
            Ok(status) => match status.next_crank {
                None => Cow::Borrowed("No additional work found waiting."),
                Some(work) => {
                    recv.trigger.trigger_crank(market, market_id).await;
                    format!("Found additional work, scheduling next crank: {work:?}").into()
                }
            },
            Err(e) => format!("Failed getting status to check for new crank work: {e:?}.").into(),
        };

        Ok(WatchedTaskOutput::new(
            match run_result {
                RunResult::NormalRun(txres) => format!(
                    "Successfully turned the crank for market {market} in transaction {}. {}",
                    txres.txhash, more_work
                ),
                RunResult::RunWithOracle(txres) => format!(
                    "Successfully updated oracles and turned the crank for market {market} in transaction {}. {}",
                    txres.txhash, more_work
                ),
                RunResult::OutOfGas => format!(
                    "Got an 'out of gas' code 11 when trying to crank. {more_work}"
                )
            }).skip_delay()
        )
    }

    async fn try_crank_with_oracle(
        &self,
        market: Address,
        crank_wallet: &Wallet,
        rewards: Option<RawAddr>,
    ) -> Result<TxResponse> {
        let market_contract = MarketContract::new(self.cosmos.make_contract(market));
        let status = market_contract.status().await?;
        let (pyth_network, pyth_oracle) = get_pyth_network(&status.config.spot_price)?;

        let mut builder = TxBuilder::default();

        let factory = self.get_factory_info().await;
        let offchain_price_data = OffchainPriceData::load(self, &factory.markets).await?;
        let update_msg = get_oracle_update_msg(
            match pyth_network {
                PythPriceServiceNetwork::Stable => &offchain_price_data.stable_ids,
                PythPriceServiceNetwork::Edge => &offchain_price_data.edge_ids,
            },
            crank_wallet,
            match pyth_network {
                PythPriceServiceNetwork::Stable => &self.endpoint_stable,
                PythPriceServiceNetwork::Edge => &self.endpoint_edge,
            },
            &self.client,
            &self.cosmos.make_contract(pyth_oracle),
        )
        .await?;
        builder.add_message(update_msg);

        // Do 0 execs, consider this an extreme case where we simply want to make sure some price gets added immediately
        builder.add_execute_message(
            market,
            crank_wallet,
            vec![],
            MarketExecuteMsg::Crank {
                execs: Some(0),
                rewards,
            },
        )?;

        let tx = builder
            .sign_and_broadcast(&self.cosmos, crank_wallet)
            .await
            .with_context(|| {
                format!(
                    "Unable to update oracle and turn crank for market {} ({market})",
                    status.market_id
                )
            });
        if let Ok(txres) = &tx {
            let mut gas_used = self.gas_usage.write().await;
            gas_used
                .entry(crank_wallet.get_address())
                .or_insert_with(|| GasUsage {
                    total: Default::default(),
                    entries: Default::default(),
                    usage_per_hour: Default::default(),
                })
                .add_entry(Utc::now(), txres.gas_used);
        }
        tx
    }
}

fn get_pyth_network(spot_price: &SpotPriceConfig) -> Result<(PythPriceServiceNetwork, Address)> {
    let (pyth, feeds, feeds_usd) = match spot_price {
        SpotPriceConfig::Manual { .. } => {
            anyhow::bail!("Manual oracle used, no Pyth updates possible")
        }
        SpotPriceConfig::Oracle {
            pyth,
            feeds,
            feeds_usd,
            ..
        } => (pyth, feeds, feeds_usd),
    };

    let uses_pyth = feeds
        .iter()
        .chain(feeds_usd.iter())
        .any(|feed| match &feed.data {
            SpotPriceFeedData::Constant { .. } => false,
            SpotPriceFeedData::Pyth { .. } => true,
            SpotPriceFeedData::Stride { .. } => false,
            SpotPriceFeedData::Sei { .. } => false,
            SpotPriceFeedData::Simple { .. } => false,
        });

    anyhow::ensure!(uses_pyth, "This market doesn't use Pyth");

    let pyth = pyth
        .as_ref()
        .context("Pyth feeds found but no pyth config found")?;

    Ok((pyth.network, pyth.contract_address.as_str().parse()?))
}
