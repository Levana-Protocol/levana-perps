use anyhow::{ensure, Result};
use axum::async_trait;
use cosmos::Cosmos;
use perps_exes::prelude::*;
use std::sync::Arc;

use crate::{
    config::BotConfigMainnet,
    util::markets::Market,
    watcher::{TaskLabel, WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, App, AppBuilder};

#[derive(Clone)]
pub(super) struct LiquidityTransaction {
    mainnet: Arc<BotConfigMainnet>,
}

impl AppBuilder {
    pub(super) fn start_liquidity_transaction_alert(
        &mut self,
        mainnet: Arc<BotConfigMainnet>,
    ) -> Result<()> {
        self.watch_periodic(
            TaskLabel::LiqudityTransactionAlert,
            LiquidityTransaction { mainnet },
        )
    }
}

#[async_trait]
impl WatchedTaskPerMarket for LiquidityTransaction {
    async fn run_single_market(
        &mut self,
        app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        check_liquidity_transaction_alert(&app.cosmos, &market.market, &self.mainnet).await
    }
}

async fn check_liquidity_transaction_alert(
    cosmos: &Cosmos,
    market: &MarketContract,
    mainnet: &BotConfigMainnet,
) -> Result<WatchedTaskOutput> {
    let latest_block = cosmos.get_latest_block_info().await?;
    let latest_height = latest_block.height;

    let latest_stats = market.status().await?;

    // Go back around 500 blocks
    const BLOCK_HEIGHTS: i64 = 500;
    let historical_height = (latest_height - BLOCK_HEIGHTS).try_into()?;
    ensure!(
        historical_height > 0,
        "Blockchain hasn't yet reached height of 500"
    );

    let historical_status = market.status_at_height(historical_height).await?;

    enum DeltaChange {
        RiseUp,
        RiseDown,
    }

    let diff_unlocked_collateral =
        latest_stats.liquidity.unlocked - historical_status.liquidity.unlocked;
    let change_type = if diff_unlocked_collateral > Collateral::zero() {
        DeltaChange::RiseUp
    } else {
        DeltaChange::RiseDown
    };
    let diff_unlocked_collateral = diff_unlocked_collateral
        .into_decimal256()
        .into_signed()
        .abs();
    if mainnet.liquidity_transaction.collateral_delta >= diff_unlocked_collateral {
        let msg = match change_type {
            DeltaChange::RiseUp => "increased",
            DeltaChange::RiseDown => "decreased",
        };
        Ok(WatchedTaskOutput { skip_delay: false, message: format!("Unlocked collateral {msg} by {diff_unlocked_collateral} between height {historical_height} and {latest_height}")})
    } else {
        Ok(WatchedTaskOutput { skip_delay: false, message: format!("Unlocked collateral between heights of {BLOCK_HEIGHTS} is under the expected delta")})
    }
}
