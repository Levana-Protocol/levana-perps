use std::fmt::Display;

use anyhow::Result;
use axum::Extension;
use msg::{
    contracts::market::entry::StatusResp,
    prelude::{MarketId, UnsignedDecimal},
};

use crate::{app::App, market_contract::MarketContract};

pub(crate) async fn markets(app: Extension<App>) -> String {
    match go(&app).await {
        Ok(x) => x.to_string(),
        Err(e) => format!("{e:?}"),
    }
}

struct Markets(Vec<(MarketId, StatusResp)>);

impl Display for Markets {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for (market_id, status) in &self.0 {
            writeln!(f, "== {market_id} == ")?;

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
                status.liquidity.locked.into_decimal256()
                    / status.liquidity.total_collateral().into_decimal256()
            )?;

            writeln!(f, "Total long  interest (in USD): {}", status.long_usd)?;
            writeln!(f, "Total short interest (in USD): {}", status.short_usd)?;

            writeln!(f, "Protocol fees collected: {}", status.fees.protocol)?;
            writeln!(f, "\n\n")?;
        }
        Ok(())
    }
}

async fn go(app: &App) -> Result<Markets> {
    let mut markets = Markets(vec![]);
    for (market_id, market_addr) in &app.get_factory_info().markets {
        let market = MarketContract::new(app.cosmos.make_contract(*market_addr));
        let status = market.status().await?;
        markets.0.push((market_id.clone(), status));
    }
    Ok(markets)
}
