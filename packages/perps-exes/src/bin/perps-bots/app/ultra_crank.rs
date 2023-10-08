use std::sync::Arc;

use anyhow::{Context, Result};
use axum::async_trait;
use chrono::Utc;
use cosmos::Wallet;
use msg::contracts::market;
use perps_exes::prelude::MarketContract;
use perps_exes::timestamp_to_date_time;

use crate::config::BotConfigTestnet;
use crate::util::markets::Market;
use crate::watcher::{WatchedTaskOutput, WatchedTaskPerMarket};

use super::factory::FactoryInfo;
use super::gas_check::GasCheckWallet;
use super::{App, AppBuilder};

#[derive(Clone)]
struct Worker {
    wallet: Wallet,
    activated: bool,
    testnet: Arc<BotConfigTestnet>,
}

/// Start the background thread to monitor and use ultra cranking.
impl AppBuilder {
    pub(super) fn start_ultra_crank_bot(&mut self, testnet: &Arc<BotConfigTestnet>) -> Result<()> {
        let ultra_crank_wallets = testnet.ultra_crank_wallets.clone();
        for (index, wallet) in ultra_crank_wallets.into_iter().enumerate() {
            // People like things that start at 1, not 0
            let index = index + 1;
            self.refill_gas(
                testnet,
                *wallet.address(),
                GasCheckWallet::UltraCrank(index),
            )?;
            let worker = Worker {
                wallet,
                activated: false,
                testnet: testnet.clone(),
            };
            self.watch_periodic(crate::watcher::TaskLabel::UltraCrank { index }, worker)?;
        }
        Ok(())
    }
}

#[async_trait]
impl WatchedTaskPerMarket for Worker {
    async fn run_single_market(
        &mut self,
        app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        app.ultra_crank(
            &market.market,
            &self.wallet,
            &mut self.activated,
            &self.testnet,
        )
        .await
    }
}

impl App {
    async fn ultra_crank(
        &self,
        market: &MarketContract,
        wallet: &Wallet,
        activated: &mut bool,
        testnet: &BotConfigTestnet,
    ) -> Result<WatchedTaskOutput> {
        let market::entry::StatusResp {
            next_crank,
            last_crank_completed,
            ..
        } = market.status().await?;
        if next_crank.is_none() {
            *activated = false;
            return Ok(WatchedTaskOutput {
                skip_delay: false,
                message: "No crank messages waiting".to_owned(),
            });
        }
        let last_crank_completed = last_crank_completed.context("No cranks have completed")?;
        let last_crank_completed = timestamp_to_date_time(last_crank_completed)?;
        let age = Utc::now()
            .signed_duration_since(last_crank_completed)
            .num_seconds();
        if age >= testnet.seconds_till_ultra.into() {
            *activated = true;
        } else if !*activated {
            return Ok(WatchedTaskOutput {
                skip_delay: false,
                message: format!("Crank is only {age} seconds out of date, not doing anything"),
            });
        }
        let res = market
            .crank_single(wallet, None, self.config.get_crank_rewards_wallet())
            .await?;
        Ok(WatchedTaskOutput {
            skip_delay: true,
            message: format!("Completed an ultracrank in {}", res.txhash),
        })
    }
}
