use anyhow::Result;
use axum::async_trait;
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
            .map(|()| WatchedTaskOutput {
                skip_delay: false,
                message: "Market is not stale".to_owned(),
            })
    }
}

async fn check_stale_single(cosmos: &Cosmos, addr: Address) -> Result<()> {
    let market = MarketContract::new(cosmos.make_contract(addr));
    let status = market.status().await?;
    if status.is_stale() {
        Err(anyhow!("Protocol is in stale state"))
    } else {
        Ok(())
    }
}
