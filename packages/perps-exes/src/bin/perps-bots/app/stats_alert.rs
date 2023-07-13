use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Cosmos};
use msg::prelude::*;
use perps_exes::contracts::MarketContract;

use crate::{
    config::BotConfigMainnet,
    watcher::{TaskLabel, WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, App, AppBuilder};

impl AppBuilder {
    pub(super) fn start_stats_alert(&mut self, mainnet: Arc<BotConfigMainnet>) -> Result<()> {
        self.watch_periodic(TaskLabel::StatsAlert, StatsAlert { mainnet })
    }
}

#[derive(Clone)]
struct StatsAlert {
    mainnet: Arc<BotConfigMainnet>,
}

#[async_trait]
impl WatchedTaskPerMarket for StatsAlert {
    async fn run_single_market(
        &mut self,
        app: &App,
        _factory: &FactoryInfo,
        _market: &MarketId,
        addr: Address,
    ) -> Result<WatchedTaskOutput> {
        check_stats_alert(&app.cosmos, addr, &self.mainnet)
            .await
            .map(|()| WatchedTaskOutput {
                skip_delay: false,
                message: "Market stats are within acceptable parameters".to_owned(),
            })
    }
}

async fn check_stats_alert(
    cosmos: &Cosmos,
    addr: Address,
    mainnet: &BotConfigMainnet,
) -> Result<()> {
    let market = MarketContract::new(cosmos.make_contract(addr));
    let status = market.status().await?;

    let total = status.liquidity.total_collateral();

    anyhow::ensure!(!total.is_zero(), "No liquidity in the market");

    let util = status
        .liquidity
        .locked
        .into_decimal256()
        .checked_div(total.into_decimal256())?;

    if util < mainnet.low_util_ratio {
        Err(anyhow::anyhow!(
            "Utilization ratio too low. Want at least {}, but found {util}",
            mainnet.low_util_ratio
        ))
    } else if util > mainnet.high_util_ratio {
        Err(anyhow::anyhow!(
            "Utilization ratio too high. Want at most {}, but found {util}",
            mainnet.high_util_ratio
        ))
    } else {
        Ok(())
    }
}
