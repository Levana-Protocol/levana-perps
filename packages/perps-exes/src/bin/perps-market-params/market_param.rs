use std::{sync::Arc, time::Duration};

use anyhow::Context;

use crate::{
    cli::{MarketId, ServeOpt},
    coingecko::{get_exchanges, CoingeckoApp, ExchangeInfo, ExchangeKind},
    slack::HttpApp,
    web::NotifyApp,
};

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct MarketsConfig {
    pub(crate) markets: Vec<MarketStatus>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct MarketStatus {
    pub(crate) status: Market,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Market {
    pub(crate) config: MarketParam,
    pub(crate) market_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct MarketParam {
    pub(crate) delta_neutrality_fee_sensitivity: Option<f64>,
}

impl MarketsConfig {
    pub(crate) fn get_dnf(&self, market_id: &MarketId) -> Option<f64> {
        self.markets
            .iter()
            .find(|item| item.status.market_id.to_lowercase() == market_id.base_quote().to_lowercase())
            .and_then(|item| item.status.config.delta_neutrality_fee_sensitivity)
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
    let market_ids = opt.market_ids;
    let http_app = HttpApp::new(opt.slack_webhook);

    loop {
        let market_config = http_app.fetch_market_status(&opt.mainnet_factories[..])?;
        for market_id in &market_ids {
            tracing::info!("Going to compute DNF for {market_id:?}");
            let configured_dnf = market_config
                .get_dnf(market_id)
                .context(format!("No DNF configured for {market_id:?}"))?;
            let exchanges = get_exchanges(&coingecko_app, *market_id)?;
            tracing::info!(
                "Total exchanges found: {} for {market_id:?}",
                exchanges.len()
            );
            let dnf = compute_dnf_sensitivity(exchanges)?;
            let diff = (configured_dnf - dnf).abs() * 100.0;
            let percentage_diff = diff / configured_dnf;
            tracing::info!("Percentage DNF deviation for {market_id}: {percentage_diff} %");
            if percentage_diff > opt.dnf_threshold {
                tracing::info!("Going to send Slack notification");
                http_app.send_notification(
                    format!("Detected DNF change for {market_id:?}"),
                    format!("Deviation: {percentage_diff} %"),
                )?;
            }
            tracing::info!("Finished computing DNF for {market_id:?}: {dnf}");
            app.dnf.write().insert(*market_id, dnf);
        }
        tracing::info!("Going to sleep 24 hours");
        std::thread::sleep(Duration::from_secs(60 * 60 * 24));
    }
}
