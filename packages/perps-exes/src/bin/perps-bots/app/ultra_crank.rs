use std::sync::Arc;

use anyhow::{Context, Result};
use axum::async_trait;
use chrono::Utc;
use cosmos::HasAddress;
use perps_exes::prelude::MarketContract;
use perpswap::contracts::market;

use crate::config::BotConfigTestnet;
use crate::util::markets::Market;
use crate::watcher::{WatchedTaskOutput, WatchedTaskPerMarket};

use super::factory::FactoryInfo;
use super::{App, AppBuilder};

#[derive(Clone)]
struct Worker {
    activated: bool,
    testnet: Arc<BotConfigTestnet>,
}

/// Start the background thread to monitor and use ultra cranking.
impl AppBuilder {
    pub(super) fn start_ultra_crank_bot(&mut self, testnet: &Arc<BotConfigTestnet>) -> Result<()> {
        for index in 1..=testnet.ultra_crank_tasks {
            let worker = Worker {
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
        app.ultra_crank(&market.market, &mut self.activated, &self.testnet)
            .await
    }
}

impl App {
    async fn ultra_crank(
        &self,
        market: &MarketContract,
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
            return Ok(WatchedTaskOutput::new("No crank messages waiting"));
        }
        let last_crank_completed = last_crank_completed
            .context("No cranks have completed")?
            .try_into_chrono_datetime()?;
        let age = Utc::now()
            .signed_duration_since(last_crank_completed)
            .num_seconds();
        if age >= testnet.seconds_till_ultra.into() {
            *activated = true;
        } else if !*activated {
            return Ok(WatchedTaskOutput::new(format!(
                "Crank is only {age} seconds out of date, not doing anything"
            )));
        }
        let res = market
            .crank_single(
                &*self.get_pool_wallet().await,
                None,
                self.config
                    .get_crank_rewards_wallet()
                    .map(|a| a.get_address_string().into()),
            )
            .await?;
        Ok(WatchedTaskOutput::new(format!(
            "Completed an ultracrank in {}",
            res.txhash
        )))
    }
}
