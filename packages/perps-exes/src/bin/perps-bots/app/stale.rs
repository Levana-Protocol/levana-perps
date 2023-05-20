use anyhow::Result;
use axum::async_trait;
use chrono::{TimeZone, Utc};
use cosmos::{Address, Cosmos};
use msg::prelude::*;
use perps_exes::contracts::MarketContract;

use crate::watcher::{TaskLabel, WatchedTaskOutput, WatchedTaskPerMarket};

use super::{factory::FactoryInfo, App, AppBuilder};

impl AppBuilder {
    pub(super) fn track_stale(&mut self) -> Result<()> {
        self.watch_periodic(TaskLabel::Stale, Stale)
    }
}

#[derive(Clone)]
struct Stale;

#[async_trait]
impl WatchedTaskPerMarket for Stale {
    async fn run_single_market(
        &self,
        app: &App,
        _factory: &FactoryInfo,
        _market: &MarketId,
        addr: Address,
    ) -> Result<WatchedTaskOutput> {
        check_stale_single(&app.cosmos, addr)
            .await
            .map(|message| WatchedTaskOutput {
                skip_delay: false,
                message,
            })
    }
}

async fn check_stale_single(cosmos: &Cosmos, addr: Address) -> Result<String> {
    let market = MarketContract::new(cosmos.make_contract(addr));
    let status = market.status().await?;
    let last_crank_completed = status
        .last_crank_completed
        .context("No cranks completed yet")?;
    let last_crank_completed = cosmwasm_std::Timestamp::from(last_crank_completed);
    let last_crank_completed = Utc
        .timestamp_opt(last_crank_completed.seconds().try_into()?, 0)
        .single()
        .context("Could not convert last_crank_completed into DateTime<Utc>")?;
    if status.is_stale() {
        Err(anyhow!(
            "Protocol is in stale state. Last completed crank timestamp: {}",
            last_crank_completed
        ))
    } else if status.congested {
        Err(anyhow!(
            "Protocol is congested, unpend queue size: {}. Maximum allowed size: {}. Last completed crank timestamp: {}",
            status.unpend_queue_size,
            status.config.unpend_limit,
            last_crank_completed
        ))
    } else {
        Ok(format!(
            "Protocol is neither stale nor congested. Last completed crank timestamp: {}. Unpend queue size: {}/{}.",
            last_crank_completed,
            status.unpend_queue_size,
            status.config.unpend_limit,
        ))
    }
}
