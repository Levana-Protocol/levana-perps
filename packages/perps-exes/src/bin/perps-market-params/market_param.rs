use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{anyhow, Context};

use crate::{
    coingecko::{get_exchanges, market_config_key, Coin, CoingeckoApp, ExchangeInfo, ExchangeKind},
    web::NotifyApp,
};

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
            value.and_then(|item| item.config.delta_neutrality_fee_sensitivity)
        }
        None => None,
    }
}

pub(crate) fn compute_dnf_sensitivity(exchanges: Vec<ExchangeInfo>) -> anyhow::Result<f64> {
    // Algorithm: https://staff.levana.finance/new-market-checklist#dnf-sensitivity
    let exchanges = exchanges.iter().filter(|exchange| {
        exchange.kind != ExchangeKind::Dex
            || exchange.name.to_lowercase() != "htx"
            || !exchange.stale
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

pub(crate) fn compute_coin_dnfs(app: Arc<NotifyApp>, coins: Vec<Coin>) -> anyhow::Result<()> {
    let coingecko_app = CoingeckoApp::new()?;
    let market_config = include_bytes!("../../../assets/market-config-updates.yaml");
    let market_config = load_markets_config(market_config)?;

    loop {
        for coin in &coins {
            tracing::info!("Going to compute DNF for {coin:?}");
            let configured_dnf = get_current_dnf(&market_config, &coin)
                .context(format!("No DNF configured for {coin:?}"))?;
            let exchanges = get_exchanges(&coingecko_app, coin.clone())?;
            let dnf = compute_dnf_sensitivity(exchanges)?;
            // todo: slack alert

            tracing::info!("Finished computing DNF for {coin:?}: {dnf}");
            app.dnf.write().insert(coin.clone(), dnf);
        }
        tracing::info!("Going to sleep 24 hours");
        std::thread::sleep(Duration::from_secs(60 * 60 * 24));
    }
    unreachable!("Unexpected finish in compute_coin_dnfs")
}
