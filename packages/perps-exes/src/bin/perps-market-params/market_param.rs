use std::collections::HashMap;

use anyhow::Context;

use crate::coingecko::{market_config_key, Coin, ExchangeInfo, ExchangeKind};

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct MarketsConfig {
    pub(crate) markets: HashMap<String, Market>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Market {
    pub(crate) config: MarketParam,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct MarketParam {
    pub(crate) delta_neutrality_fee_sensitivity: Option<f64>,
}

pub(crate) fn load_markets_config(data: &[u8]) -> anyhow::Result<MarketsConfig> {
    let result = serde_yaml::from_slice(data)?;
    Ok(result)
}

pub(crate) fn get_current_dnf(market: &MarketsConfig, coin: &Coin) -> Option<f64> {
    let key = market_config_key(coin);
    match key {
        Some(key) => {
            let value = market.markets.get(&key);
            value
                .map(|item| item.config.delta_neutrality_fee_sensitivity)
                .flatten()
        }
        None => None,
    }
}

pub(crate) fn compute_dnf_sensitivity(exchanges: Vec<ExchangeInfo>) -> anyhow::Result<f64> {
    // Algorithm: https://staff.levana.finance/new-market-checklist#dnf-sensitivity
    let exchanges = exchanges.iter().filter(|exchange| {
        exchange.kind != ExchangeKind::Dex
            || exchange.name.to_lowercase() != "htx"
            || exchange.stale != true
    });
    let max_volume_exchange = exchanges
        .clone()
        .max_by(|a, b| a.twenty_four_volume.total_cmp(&b.twenty_four_volume))
        .context("No max value found")?;
    let total_volume_percentage = exchanges
        .map(|exchange| exchange.volume_percentage.unwrap_or_default())
        .sum::<f64>();
    let market_share = max_volume_exchange
        .volume_percentage
        .context("Exchange with maximum volume doesn't have metric")?
        / total_volume_percentage;
    let min_depth_liquidity = max_volume_exchange
        .negative_two_depth
        .min(max_volume_exchange.positive_two_depth);
    let dnf = (min_depth_liquidity / market_share) * 25.0;
    Ok(dnf)
}
