use anyhow::Result;
use axum::async_trait;
use msg::{contracts::market::entry::StatusResp, prelude::*};

use crate::{
    util::markets::Market,
    watcher::{TaskLabel, WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, App, AppBuilder};

impl AppBuilder {
    pub(super) fn track_stats(&mut self) -> Result<()> {
        self.watch_periodic(TaskLabel::Stats, Stats)
    }
}

#[derive(Clone)]
struct Stats;

#[async_trait]
impl WatchedTaskPerMarket for Stats {
    async fn run_single_market(
        &mut self,
        _app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        let status = market.market.status().await?;
        let market_stats = MarketStats { status };
        Ok(WatchedTaskOutput::new(market_stats.to_string()))
    }
}

struct MarketStats {
    status: StatusResp,
}

impl Display for MarketStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let MarketStats { status } = self;
        let total_collateral = status.liquidity.total_collateral();
        writeln!(f)?;
        writeln!(f, "Total locked   liquidity: {}", status.liquidity.locked)?;
        writeln!(f, "Total unlocked liquidity: {}", status.liquidity.unlocked)?;
        writeln!(
            f,
            "Total          liquidity: {}",
            match status.liquidity.total_collateral() {
                Ok(value) => value.to_string(),
                Err(_) => "overflow".into(),
            }
        )?;
        writeln!(f, "Utilization ratio: {}", {
            match total_collateral {
                Ok(value) => status
                    .liquidity
                    .locked
                    .into_decimal256()
                    .checked_div(value.into_decimal256())
                    .ok()
                    .unwrap_or_default()
                    .to_string(),
                Err(_) => "overflow".into(),
            }
        })?;

        writeln!(f, "Total long  interest (in USD): {}", status.long_usd)?;
        writeln!(f, "Total short interest (in USD): {}", status.short_usd)?;

        writeln!(f, "Protocol fees collected: {}", status.fees.protocol)?;
        writeln!(f, "Borrow fee total: {}", status.borrow_fee)?;
        writeln!(f, "Borrow fee LP   : {}", status.borrow_fee_lp)?;
        writeln!(f, "Borrow fee xLP  : {}", status.borrow_fee_xlp)?;
        Ok(())
    }
}
