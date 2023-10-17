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

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::async_trait;
use chrono::Utc;
use cosmos::{Address, HasAddress, TxBuilder, Wallet};
use msg::prelude::MarketExecuteMsg;

use crate::config::BotConfigByType;
use crate::watcher::{Heartbeat, WatchedTask, WatchedTaskOutput};

use super::gas_check::GasCheckWallet;
use super::{App, AppBuilder};

pub(crate) struct CrankNeeded {
    market_contract: Address,
}

#[derive(Clone)]
pub(crate) struct TriggerCrank {
    send: Arc<tokio::sync::mpsc::Sender<CrankNeeded>>,
}

impl TriggerCrank {
    pub(crate) async fn trigger_crank(&self, contract: Address) -> Result<()> {
        self.send
            .send(CrankNeeded {
                market_contract: contract,
            })
            .await
            .context("Failed to trigger a crank, this indicates a code bug in crank_run")
    }
}

struct Worker {
    crank_wallet: Wallet,
    recv: tokio::sync::mpsc::Receiver<CrankNeeded>,
}

/// Start the background thread to turn the crank on the crank bots.
impl AppBuilder {
    pub(super) fn start_crank_run(&mut self) -> Result<Option<TriggerCrank>> {
        if let Some(crank_wallet) = self.app.config.crank_wallet.clone() {
            match &self.app.config.by_type {
                BotConfigByType::Testnet { inner } => {
                    let inner = inner.clone();
                    self.refill_gas(&inner, *crank_wallet.address(), GasCheckWallet::Crank)?
                }
                BotConfigByType::Mainnet { inner } => self.alert_on_low_gas(
                    *crank_wallet.address(),
                    GasCheckWallet::Crank,
                    inner.min_gas_crank,
                )?,
            }

            let (send, recv) = tokio::sync::mpsc::channel(50);

            let worker = Worker { crank_wallet, recv };
            self.watch_periodic(crate::watcher::TaskLabel::CrankRun, worker)?;
            Ok(Some(TriggerCrank {
                send: Arc::new(send),
            }))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl WatchedTask for Worker {
    async fn run_single(&mut self, app: &App, _: Heartbeat) -> Result<WatchedTaskOutput> {
        app.crank(&self.crank_wallet, &mut self.recv).await
    }
}

const CRANK_EXECS: &[u32] = &[30, 25, 20, 15, 10, 7, 6, 5, 4, 3, 2, 1];

impl App {
    async fn crank(
        &self,
        crank_wallet: &Wallet,
        recv: &mut tokio::sync::mpsc::Receiver<CrankNeeded>,
    ) -> Result<WatchedTaskOutput> {
        const MAX_WAIT_SECONDS: u64 = 20;
        const MAX_CRANKS_PER_TX: usize = 3;

        // Wait for up to 20 seconds for new work to appear. If it doesn't, update our status message that no cranking was needed.
        let crank_needed = tokio::time::timeout(
            tokio::time::Duration::from_secs(MAX_WAIT_SECONDS),
            recv.recv(),
        )
        .await;
        let crank_needed = match crank_needed {
            Err(_) => {
                return Ok(WatchedTaskOutput {
                    // Irrelevant, no delay here
                    skip_delay: false,
                    message: "No crank work needed".to_owned(),
                });
            }
            Ok(None) => anyhow::bail!(
                "Impossible None returned from crank needed queue, this indicates a code bug"
            ),
            Ok(Some(crank_needed)) => crank_needed,
        };

        // Get a few more work items, but we don't want to crank the same market multiple times.
        let mut markets = HashSet::new();
        markets.insert(crank_needed.market_contract);
        while markets.len() < MAX_CRANKS_PER_TX {
            match recv.try_recv() {
                Ok(crank_needed) => {
                    markets.insert(crank_needed.market_contract);
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => anyhow::bail!("Impossible Disconnected returned from crank needed queue, this indicates a code bug"),
            }
        }

        let rewards = self.config.get_crank_rewards_wallet();
        for execs in CRANK_EXECS {
            let crank_start = Utc::now();
            let res = self
                .try_with_execs(crank_wallet, &markets, Some(*execs), rewards)
                .await;
            let crank_time = Utc::now() - crank_start;
            log::debug!("Crank for {execs} takes {crank_time}");
            match res {
                Ok(x) => return Ok(x),
                Err(e) => log::warn!("Cranking with execs=={execs} failed: {e:?}"),
            }
        }

        let crank_start = Utc::now();
        let res = self
            .try_with_execs(crank_wallet, &markets, None, rewards)
            .await;
        let crank_time = Utc::now() - crank_start;
        log::debug!("Crank for None takes {crank_time}");
        res
    }

    async fn try_with_execs(
        &self,
        crank_wallet: &Wallet,
        markets: &HashSet<Address>,
        execs: Option<u32>,
        rewards: Option<Address>,
    ) -> Result<WatchedTaskOutput> {
        let mut builder = TxBuilder::default();

        for market in markets {
            builder.add_execute_message_mut(
                *market,
                crank_wallet,
                vec![],
                MarketExecuteMsg::Crank {
                    execs,
                    rewards: rewards.map(|a| a.get_address_string().into()),
                },
            )?;
        }

        let txres = builder
            .sign_and_broadcast(&self.cosmos, crank_wallet)
            .await?;
        Ok(WatchedTaskOutput {
            skip_delay: true,
            message: format!(
                "Successfully turned the crank for markets {markets:?} in transaction {}",
                txres.txhash
            ),
        })
    }
}
