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
use std::time::Instant;

use anyhow::{Context, Result};
use axum::async_trait;

use chrono::Duration;
use cosmos::proto::cosmos::base::abci::v1beta1::TxResponse;
use cosmos::{Address, HasAddress, TxBuilder, Wallet};
use msg::prelude::MarketExecuteMsg;
use perps_exes::prelude::{MarketContract, MarketId};

use crate::app::CrankTriggerReason;
use crate::util::misc::track_tx_fees;
use crate::watcher::{Heartbeat, WatchedTask, WatchedTaskOutput};

use self::trigger_crank::{CrankReceiver, CrankWorkItem};

use super::gas_check::GasCheckWallet;
use super::{App, AppBuilder, GasLevel};
pub(crate) use trigger_crank::TriggerCrank;

struct Worker {
    crank_wallet: Wallet,
    recv: CrankReceiver,
}
pub(crate) enum RunResult {
    NormalRun(TxResponse),
    OutOfGas,
    OsmosisEpoch(anyhow::Error),
    OsmosisCongested(anyhow::Error),
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
        app.crank_receive(&self.crank_wallet, &self.recv).await
    }
}

const CRANK_EXECS: &[u32] = &[7, 4, 1];

impl App {
    async fn crank_receive(
        &self,
        crank_wallet: &Wallet,
        recv: &CrankReceiver,
    ) -> Result<WatchedTaskOutput> {
        let CrankWorkItem {
            address: market,
            id: market_id,
            guard: crank_guard,
            reason,
            queued,
            received,
        } = recv.receive_work().await?;
        let start_crank = Instant::now();
        let run_result = self
            .crank(crank_wallet, market, &market_id, reason, None)
            .await?;

        // Successfully cranked, check if there's more work and, if so, schedule it to be started again
        std::mem::drop(crank_guard);

        let more_work = match MarketContract::new(self.cosmos.make_contract(market))
            .status()
            .await
        {
            Ok(status) => match status.next_crank {
                None => Cow::Borrowed("No additional work found waiting."),
                Some(work) => {
                    recv.trigger
                        .trigger_crank(market, market_id, CrankTriggerReason::MoreWorkFound)
                        .await;
                    format!("Found additional work, scheduling next crank: {work:?}").into()
                }
            },
            Err(e) => format!("Failed getting status to check for new crank work: {e:?}.").into(),
        };

        let output = match run_result {
            RunResult::NormalRun(txres) => {
                let message =
                format!(
                    "Successfully turned the crank for market {market} in transaction {}. {}. Queued delay: {:?}, Elapsed since starting to crank: {:?}",
                    txres.txhash, more_work, received.saturating_duration_since(queued), start_crank.elapsed(),
                );
                WatchedTaskOutput::new(message)
            }
            RunResult::OutOfGas => {
                let message =
                    format!("Got an 'out of gas' code 11 when trying to crank. {more_work}");
                WatchedTaskOutput::new(message)
                    .set_expiry(Duration::seconds(10))
                    .set_error()
            }
            RunResult::OsmosisEpoch(e) => {
                let message = format!("Ignoring crank run error since we think we're in the Osmosis epoch, error: {e:?}");
                WatchedTaskOutput::new(message)
            }
            RunResult::OsmosisCongested(e) => {
                let message = format!("Ignoring crank run error since we think the Osmosis chain is overly congested, error: {e:?}");
                WatchedTaskOutput::new(message)
            }
        };

        Ok(output.skip_delay())
    }

    pub(crate) async fn crank(
        &self,
        crank_wallet: &Wallet,
        market: Address,
        market_id: &MarketId,
        reason: CrankTriggerReason,
        // an array of N execs to try with fallbacks
        execs: Option<&[u32]>,
    ) -> Result<RunResult> {
        let cosmos = match reason.gas_level() {
            GasLevel::Normal => &self.cosmos,
            // we won't use the very high gas wallet in cranking, that's reserved for the high gas task
            GasLevel::High | GasLevel::VeryHigh => &self.cosmos_high_gas,
        };

        let rewards = self
            .config
            .get_crank_rewards_wallet()
            .map(|a| a.get_address_string().into());

        let mut actual_execs = None;

        // Simulate decreasing numbers of execs until we find one that looks like it will pass.
        for execs in execs.unwrap_or(CRANK_EXECS) {
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
                .simulate(cosmos, &[crank_wallet.get_address()])
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
        builder.set_memo(reason.to_string());

        match builder
            .sign_and_broadcast_cosmos_tx(cosmos, crank_wallet)
            .await
            .with_context(|| format!("Unable to turn crank for market {market_id} ({market})"))
        {
            Ok(txres) => {
                track_tx_fees(self, crank_wallet.get_address(), &txres).await;
                Ok(RunResult::NormalRun(txres.response))
            }
            Err(e) => {
                if self.is_osmosis_epoch() {
                    Ok(RunResult::OsmosisEpoch(e))
                } else if self.get_congested_info().await.is_congested() {
                    Ok(RunResult::OsmosisCongested(e))
                } else {
                    let error_as_str = format!("{e:?}");

                    // If we got an "out of gas" code 11 error, we want to ignore
                    // it. This usually happens when new work comes in. The logic
                    // below to check if new work is available will cause a new
                    // crank run to be scheduled, if one is needed.
                    if error_as_str.contains("out of gas") || error_as_str.contains("code 11") {
                        Ok(RunResult::OutOfGas)
                    }
                    // We previously checked here for a price_too_old error message.
                    // However, with the new price logic from deferred execution, that should never
                    // happen. So now we'll simply allow such an error message to bubble up.
                    else {
                        Err(e)
                    }
                }
            }
        }
    }
}
