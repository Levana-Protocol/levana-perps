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
use cosmos::{Address, HasAddress, TxBuilder, Wallet};
use msg::prelude::MarketExecuteMsg;
use perps_exes::prelude::MarketContract;

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
        app.crank(&self.crank_wallet, &mut self.recv).await
    }
}

const CRANK_EXECS: &[u32] = &[30, 25, 20, 15, 10, 7, 6, 5, 4, 3, 2, 1];

impl App {
    async fn crank(
        &self,
        crank_wallet: &Wallet,
        recv: &CrankReceiver,
    ) -> Result<WatchedTaskOutput> {
        // Wait for up to 20 seconds for new work to appear. If it doesn't, update our status message that no cranking was needed.
        let crank_needed = match recv.receive_with_timeout().await {
            None => {
                return Ok(WatchedTaskOutput {
                    // Irrelevant, no delay here
                    skip_delay: false,
                    message: "No crank work needed".to_owned(),
                });
            }
            Some(crank_needed) => crank_needed,
        };

        // NOTE: in theory this approach may end up running needless cranks. The
        // reason: supposed the crank watcher sees some crank work needs to be
        // done for market X, triggers the work, and then, before the crank run
        // completes, it checks again and _still_ sees that work needs to be
        // done. It will queue up an extra crank, and we'll simply run it. We
        // could approach this in a few ways, such as clearing any pending
        // cranks from the queue for the same market after running the crank.
        // However, we'll start off conservatively and simply run each time. Due
        // to the delays in place for checking the price and crank workloads and
        // the lack of a delay here, it should be a rare occurrence. Over time,
        // we can check if there are a significant number of times that we try
        // to run a crank from the bots and no work happens. It may be worth
        // capturing those event logs here and keeping some stats on "useless
        // cranks performed."

        let rewards = self.config.get_crank_rewards_wallet();
        for execs in CRANK_EXECS {
            let crank_start = Utc::now();
            let res = self
                .try_with_execs(
                    crank_wallet,
                    crank_needed,
                    Some(*execs),
                    rewards,
                    &recv.trigger,
                )
                .await;
            let crank_time = Utc::now() - crank_start;
            tracing::debug!("Crank for {execs} takes {crank_time}");
            match res {
                Ok(x) => return Ok(x),
                Err(e) => tracing::warn!("Cranking with execs=={execs} failed: {e:?}"),
            }
        }

        let crank_start = Utc::now();
        let res = self
            .try_with_execs(crank_wallet, crank_needed, None, rewards, &recv.trigger)
            .await;
        let crank_time = Utc::now() - crank_start;
        tracing::debug!("Crank for None takes {crank_time}");
        res
    }

    async fn try_with_execs(
        &self,
        crank_wallet: &Wallet,
        market: Address,
        execs: Option<u32>,
        rewards: Option<Address>,
        trigger: &TriggerCrank,
    ) -> Result<WatchedTaskOutput> {
        let mut builder = TxBuilder::default();

        builder.add_execute_message_mut(
            market,
            crank_wallet,
            vec![],
            MarketExecuteMsg::Crank {
                execs,
                rewards: rewards.map(|a| a.get_address_string().into()),
            },
        )?;

        let txres = builder
            .sign_and_broadcast(&self.cosmos, crank_wallet)
            .await
            .with_context(|| format!("Unable to turn crank for market {market}"))?;

        // Successfully cranked, check if there's more work and, if so, schedule it to be started again
        let more_work = match MarketContract::new(self.cosmos.make_contract(market))
            .status()
            .await
        {
            Ok(status) => match status.next_crank {
                None => Cow::Borrowed("No additional work found waiting."),
                Some(work) => {
                    trigger.trigger_crank(market).await;
                    format!("Found additional work, scheduling next crank: {work:?}").into()
                }
            },
            Err(e) => format!("Failed getting status to check for new crank work: {e:?}.").into(),
        };

        Ok(WatchedTaskOutput {
            skip_delay: true,
            message: format!(
                "Successfully turned the crank for market {market} in transaction {}. {}",
                txres.txhash, more_work
            ),
        })
    }
}
