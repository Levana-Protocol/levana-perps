use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context;

use crate::{
    cli::ServeOpt,
    coingecko::{get_exchanges, market_config_key, Coin, CoingeckoApp, ExchangeInfo, ExchangeKind},
    slack::SlackApp,
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
    tracing::debug!("Total exchanges: {}", exchanges.len());
    let exchanges = exchanges.iter().filter(|exchange| {
        exchange.kind != ExchangeKind::Dex
            && exchange.name.to_lowercase() != "htx"
            && !exchange.stale
    });
    let max_volume_exchange = exchanges
        .clone()
        .max_by(|a, b| a.twenty_four_volume.total_cmp(&b.twenty_four_volume))
        .context("No max value found")?;
    tracing::debug!("Max volume exchange: {max_volume_exchange:#?}");
    let total_volume_percentage = exchanges
        .map(|exchange| exchange.volume_percentage.unwrap_or_default())
        .sum::<f64>();
    let market_share = max_volume_exchange
        .volume_percentage
        .context("Exchange with maximum volume doesn't have metric")?
        / total_volume_percentage;
    tracing::debug!("Market share: {market_share}");
    let min_depth_liquidity = max_volume_exchange
        .negative_two_depth
        .min(max_volume_exchange.positive_two_depth);
    let dnf = (min_depth_liquidity / market_share) * 25.0;
    Ok(dnf)
}

pub(crate) fn compute_coin_dnfs(app: Arc<NotifyApp>, opt: ServeOpt) -> anyhow::Result<()> {
    let coingecko_app = CoingeckoApp::new()?;
    let coins = opt.coins;
    let market_config = include_bytes!("../../../assets/market-config-updates.yaml");
    let market_config = load_markets_config(market_config)?;
    let slack = SlackApp::new(opt.slack_webhook);

    loop {
        for coin in &coins {
            tracing::info!("Going to compute DNF for {coin:?}");
            let configured_dnf = get_current_dnf(&market_config, coin)
                .context(format!("No DNF configured for {coin:?}"))?;
            let exchanges = get_exchanges(&coingecko_app, *coin)?;
            tracing::info!("Total exchanges found: {} for {coin:?}", exchanges.len());
            let dnf = compute_dnf_sensitivity(exchanges)?;
            let diff = (configured_dnf - dnf).abs() * 100.0;
            let percentage_diff = diff / configured_dnf;
            tracing::info!("Percentage DNF deviation for {coin}: {percentage_diff} %");
            if percentage_diff > opt.dnf_threshold {
                tracing::info!("Going to send Slack notification");
                slack.send_notification(
                    format!("Detected DNF change for {coin:?}"),
                    format!("Deviation: {percentage_diff} %"),
                )?;
            }
            tracing::info!("Finished computing DNF for {coin:?}: {dnf}");
            app.dnf.write().insert(*coin, dnf);
        }
        tracing::info!("Going to sleep 24 hours");
        std::thread::sleep(Duration::from_secs(60 * 60 * 24));
    }
}
