use std::{sync::Arc, time::Duration};

use anyhow::Context;
use itertools::Itertools;
use shared::storage::{MarketId, MarketType};

use crate::{
    cli::{Opt, ServeOpt},
    coingecko::{CmcMarketPair, ExchangeKind},
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
    pub(crate) delta_neutrality_fee_sensitivity: String,
}

impl MarketsConfig {
    pub(crate) fn get_chain_dnf(&self, market_id: &MarketId) -> anyhow::Result<f64> {
        let result = self
            .markets
            .iter()
            .find(|item| {
                item.status.market_id.to_lowercase() == market_id.to_string().to_lowercase()
            })
            .map(|item| item.status.config.delta_neutrality_fee_sensitivity.clone())
            .context("No dnf found")?;
        let result = result.parse()?;
        Ok(result)
    }
}

pub(crate) async fn dnf_sensitivity(
    http_app: &HttpApp,
    market_id: &MarketId,
) -> anyhow::Result<f64> {
    let quote_asset = market_id.get_quote();
    if quote_asset == "USD" || quote_asset == "USDC" {
        let exchanges = http_app.get_market_pair(market_id.clone()).await?;
        tracing::debug!(
            "Total exchanges found: {} for {market_id:?}",
            exchanges.data.market_pairs.len()
        );
        return compute_dnf_sensitivity(exchanges.data.market_pairs);
    }
    let base_asset = market_id.get_base();
    let base_market_id = MarketId::new(base_asset, "USD", MarketType::CollateralIsQuote);
    let quote_market_id = MarketId::new(quote_asset, "USD", MarketType::CollateralIsQuote);
    let base_exchanges = http_app.get_market_pair(base_market_id).await?;
    let quote_exchanges = http_app.get_market_pair(quote_market_id).await?;
    let base_dnf = compute_dnf_sensitivity(base_exchanges.data.market_pairs)?;
    let quote_dnf = compute_dnf_sensitivity(quote_exchanges.data.market_pairs)?;
    Ok(base_dnf.min(quote_dnf))
}

fn compute_dnf_sensitivity(exchanges: Vec<CmcMarketPair>) -> anyhow::Result<f64> {
    // Algorithm: https://staff.levana.finance/new-market-checklist#dnf-sensitivity
    tracing::debug!("Total exchanges: {}", exchanges.len());
    let exchanges = exchanges
        .iter()
        .filter(|exchange| {
            exchange.exchange_name.to_lowercase() != "htx" && exchange.outlier_detected > 0.3
        })
        .filter_map(|item| match item.exchange_id.exchange_type() {
            Ok(exchange_kind) => {
                if exchange_kind == ExchangeKind::Dex {
                    None
                } else {
                    Some(item)
                }
            }
            Err(err) => {
                tracing::debug!("Not able to find exchange type: {err}");
                None
            }
        });

    let max_volume_exchange = exchanges
        .clone()
        .max_by(|a, b| a.volume_24h_usd.total_cmp(&b.volume_24h_usd))
        .context("No max value found")?;
    tracing::debug!("Max volume exchange: {max_volume_exchange:#?}");
    let total_volume_percentage = exchanges
        .map(|exchange| exchange.volume_24h_usd)
        .sum::<f64>();
    let market_share = max_volume_exchange.volume_24h_usd / total_volume_percentage;
    tracing::debug!("Market share: {market_share}");
    let min_depth_liquidity = max_volume_exchange
        .depth_usd_negative_two
        .min(max_volume_exchange.depth_usd_positive_two);
    let dnf = (min_depth_liquidity / market_share) * 25.0;
    Ok(dnf)
}

pub(crate) async fn compute_coin_dnfs(
    app: Arc<NotifyApp>,
    serve_opt: ServeOpt,
    opt: Opt,
) -> anyhow::Result<()> {
    let market_ids = serve_opt.market_ids;
    let http_app = HttpApp::new(Some(serve_opt.slack_webhook), opt.cmc_key.clone());

    loop {
        tracing::info!("Going to fetch market status from querier");
        let market_config = http_app
            .fetch_market_status(&serve_opt.mainnet_factories[..])
            .await?;
        for market_id in &market_ids {
            tracing::info!("Going to compute DNF for {market_id:?}");
            let configured_dnf = market_config
                .get_chain_dnf(market_id)
                .context(format!("No DNF configured for {market_id:?}"))?;
            let dnf = dnf_sensitivity(&http_app, market_id).await?;
            let diff = (configured_dnf - dnf).abs() * 100.0;
            let percentage_diff = diff / configured_dnf;
            tracing::info!("Percentage DNF deviation for {market_id}: {percentage_diff} %");
            if percentage_diff > serve_opt.dnf_threshold {
                tracing::info!("Going to send Slack notification");
                http_app
                    .send_notification(
                        format!("Detected DNF change for {market_id:?}"),
                        format!("Deviation: {percentage_diff} %"),
                    )
                    .await?;
            }
            tracing::info!("Finished computing DNF for {market_id:?}: {dnf}");
            app.dnf.write().insert(market_id.clone(), dnf);
        }
        tracing::info!("Going to sleep 24 hours");
        tokio::time::sleep(Duration::from_secs(60 * 60 * 24)).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::coingecko::CmcMarketPair;

    #[tokio::test]
    async fn sample_dnf_computation() {
        let exchanges = vec![
            CmcMarketPair {
                exchange_id: 1,
                exchange_name: "mexc".to_owned(),
                market_id: "LVN_USD".to_owned(),
                depth_usd_negative_two: 5828.0,
                depth_usd_positive_two: 7719.0,
                volume_percent: 1.06,
                volume_24h_usd: 27304.39,
                outlier_detected: 0.6,
                center_type: crate::coingecko::ExchangeKind::Cex,
            },
            CmcMarketPair {
                exchange_id: 2,
                exchange_name: "gate.io".to_owned(),
                market_id: "LVN_USD".to_owned(),
                depth_usd_negative_two: 1756.0,
                depth_usd_positive_two: 22140.0,
                volume_percent: 0.9,
                volume_24h_usd: 23065.95,
                outlier_detected: 0.6,
                center_type: crate::coingecko::ExchangeKind::Cex,
            },
        ];
        let dnf = super::compute_dnf_sensitivity(exchanges).unwrap();
        assert_eq!(dnf.round(), 269408.0, "Expected DNF");
    }
}
