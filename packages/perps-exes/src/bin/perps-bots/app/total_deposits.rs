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
pub(super) struct DepositsTransaction {
    mainnet: Arc<BotConfigMainnet>,
}

impl AppBuilder {
    pub(super) fn start_total_deposits_alert(
        &mut self,
        mainnet: Arc<BotConfigMainnet>,
    ) -> Result<()> {
        self.watch_periodic(
            TaskLabel::TotalDepositAlert,
            DepositsTransaction { mainnet },
        )
    }
}

#[async_trait]
impl WatchedTaskPerMarket for DepositsTransaction {
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

    let historical_height =
        (latest_height - i64::from(mainnet.liquidity_transaction.number_of_blocks)).try_into()?;
    ensure!(
        historical_height > 0,
        format!(
            "Blockchain hasn't yet reached height of {}",
            mainnet.liquidity_transaction.number_of_blocks
        )
    );

    let historical_status = market.status_at_height(historical_height).await?;

    enum DeltaChange {
        RiseUp,
        RiseDown,
    }

    let diff_total_deposits = latest_stats.liquidity.total_tokens().into_signed()
        - historical_status.liquidity.total_tokens().into_signed();
    let change_type = if diff_total_deposits.is_strictly_positive() {
        DeltaChange::RiseUp
    } else {
        DeltaChange::RiseDown
    };

    let historical_total_tokens = historical_status.liquidity.total_tokens();
    // If this is not ensured, you would get divide by zero errors
    ensure!(
        historical_total_tokens.ne(&LpToken::zero()),
        "Historical tokens should be greater than zero"
    );

    let percentage_change = diff_total_deposits
        .abs_unsigned()
        .into_decimal256()
        .checked_div(historical_total_tokens.into_decimal256())?
        .checked_mul("100".parse()?)?;
    if mainnet.liquidity_transaction.total_deposits_percentage <= percentage_change {
        let msg = match change_type {
            DeltaChange::RiseUp => "increased",
            DeltaChange::RiseDown => "decreased",
        };
        Ok(WatchedTaskOutput::new(format!("Total deposits {msg} by {percentage_change}% between height {historical_height} and {latest_height}")))
    } else {
        Ok(WatchedTaskOutput::new(
            format!(
                "Total deposits between heights of {} is under the expected delta (Percentage change: {percentage_change}%)",
                mainnet.liquidity_transaction.number_of_blocks
            ),
        ))
    }
}
