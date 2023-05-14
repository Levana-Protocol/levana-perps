use anyhow::Result;
use axum::async_trait;
use cosmos::Address;
use msg::{contracts::market::entry::StatusResp, prelude::*};
use perps_exes::contracts::MarketContract;

use crate::watcher::{TaskLabel, WatchedTaskOutput, WatchedTaskPerMarket};

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
        &self,
        app: &App,
        _factory: &FactoryInfo,
        _market: &MarketId,
        addr: Address,
    ) -> Result<WatchedTaskOutput> {
        let market = MarketContract::new(app.cosmos.make_contract(addr));
        let status = market.status().await?;
        let market_stats = MarketStats { status };
        Ok(WatchedTaskOutput {
            message: market_stats.to_string(),
            skip_delay: false,
        })
    }
}

struct MarketStats {
    status: StatusResp,
}

impl Display for MarketStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let MarketStats { status } = self;
        writeln!(f)?;
        writeln!(f, "Total locked   liquidity: {}", status.liquidity.locked)?;
        writeln!(f, "Total unlocked liquidity: {}", status.liquidity.unlocked)?;
        writeln!(
            f,
            "Total          liquidity: {}",
            status.liquidity.total_collateral()
        )?;
        writeln!(
            f,
            "Utilization ratio: {}",
            status
                .liquidity
                .locked
                .into_decimal256()
                .checked_div(status.liquidity.total_collateral().into_decimal256())
                .ok()
                .unwrap_or_default()
        )?;

        writeln!(f, "Total long  interest (in USD): {}", status.long_usd)?;
        writeln!(f, "Total short interest (in USD): {}", status.short_usd)?;

        writeln!(f, "Protocol fees collected: {}", status.fees.protocol)?;
        writeln!(f, "Borrow fee total: {}", status.borrow_fee)?;
        writeln!(f, "Borrow fee LP   : {}", status.borrow_fee_lp)?;
        writeln!(f, "Borrow fee xLP  : {}", status.borrow_fee_xlp)?;
        Ok(())
    }
}
