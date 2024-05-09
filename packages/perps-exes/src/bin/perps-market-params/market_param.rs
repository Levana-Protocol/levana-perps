use std::{sync::Arc, time::Duration};

use anyhow::{bail, Context};
use shared::storage::MarketId;

use crate::{
    cli::{Opt, ServeOpt},
    coingecko::{CmcMarketPair, ExchangeId, ExchangeKind},
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
    pub(crate) market_id: MarketId,
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
            .find(|item| item.status.market_id == *market_id)
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
    tracing::debug!("Going to compute dnf_sensitivity for {market_id}");
    let base_asset = AssetName(market_id.get_base());
    let quote_asset = AssetName(market_id.get_quote());
    if quote_asset.is_usd_equiv() {
        tracing::debug!("Fetch exchanges");
        let exchanges = http_app.get_market_pair(base_asset).await?;
        tracing::debug!(
            "Total exchanges found: {} for {market_id:?}",
            exchanges.data.market_pairs.len()
        );
        let dnf_in_usd = compute_dnf_sensitivity(exchanges.data.market_pairs)?;
        return dnf_in_usd.to_asset_amount(base_asset, http_app).await;
    }
    tracing::debug!("Fetch base_exchanges");
    let base_exchanges = http_app.get_market_pair(base_asset).await?;
    tracing::debug!("Fetch quote_exchanges");
    let quote_exchanges = http_app.get_market_pair(quote_asset).await?;
    let base_dnf_in_usd = compute_dnf_sensitivity(base_exchanges.data.market_pairs)?;
    let quote_dnf_in_usd = compute_dnf_sensitivity(quote_exchanges.data.market_pairs)?;
    let dnf_in_usd = if base_dnf_in_usd > quote_dnf_in_usd {
        quote_dnf_in_usd
    } else {
        base_dnf_in_usd
    };
    dnf_in_usd.to_asset_amount(base_asset, http_app).await
}

fn is_centralized_exchange(id: &ExchangeId) -> bool {
    match id.exchange_type() {
        Ok(kind) => kind == ExchangeKind::Cex,
        Err(_) => {
            tracing::debug!("Not able to find exchange type for {id:?}");
            false
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AssetName<'a>(pub(crate) &'a str);
impl AssetName<'_> {
    /// Is the asset either USD or a stablecoin pinned to USD?
    fn is_usd_equiv(&self) -> bool {
        self.0 == "USD" || self.0 == "USDC" || self.0 == "USDT"
    }
}
#[derive(PartialOrd, PartialEq)]
struct MyUsd(f64);

impl MyUsd {
    async fn to_asset_amount(
        &self,
        asset: AssetName<'_>,
        http_app: &HttpApp,
    ) -> anyhow::Result<f64> {
        let price = http_app.get_price_in_usd(asset).await?;
        Ok(self.0 / price)
    }
}

struct DnfExchanges {
    exchanges: Vec<CmcMarketPair>,
    max_exchange: CmcMarketPair,
}

fn filter_invalid_exchanges(exchanges: Vec<CmcMarketPair>) -> anyhow::Result<DnfExchanges> {
    let exchanges = exchanges.into_iter().filter(|exchange| {
        exchange.exchange_name.to_lowercase() != "htx"
            && exchange.outlier_detected < 0.3
            && is_centralized_exchange(&exchange.exchange_id)
    });

    let max_volume_exchange = exchanges
        .clone()
        .max_by(|a, b| a.volume_24h_usd.total_cmp(&b.volume_24h_usd))
        .context("No max value found")?;

    if max_volume_exchange.depth_usd_negative_two == 0.0
        || max_volume_exchange.depth_usd_positive_two == 0.0
    {
        // Skip this exchange
        let exchanges: Vec<_> = exchanges
            .filter(|item| *item != max_volume_exchange)
            .collect();
        if exchanges.is_empty() {
            bail!("No valid exchange data found")
        } else {
            filter_invalid_exchanges(exchanges)
        }
    } else {
        Ok(DnfExchanges {
            exchanges: exchanges.collect(),
            max_exchange: max_volume_exchange,
        })
    }
}

fn compute_dnf_sensitivity(exchanges: Vec<CmcMarketPair>) -> anyhow::Result<MyUsd> {
    // Algorithm: https://staff.levana.finance/new-market-checklist#dnf-sensitivity
    tracing::debug!("Total exchanges: {}", exchanges.len());
    let dnf_exchange = filter_invalid_exchanges(exchanges)?;

    let exchanges = dnf_exchange.exchanges.iter();

    let max_volume_exchange = dnf_exchange.max_exchange;

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
    Ok(MyUsd(dnf))
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct DnfNotify {
    pub(crate) configured_dnf: f64,
    pub(crate) computed_dnf: f64,
    pub(crate) percentage_diff: f64,
    pub(crate) should_notify: bool,
}

pub(crate) fn compute_dnf_notify(
    computed_dnf: f64,
    configured_dnf: f64,
    dnf_increase_threshold: f64,
    dnf_decrease_threshold: f64,
) -> DnfNotify {
    let diff = (configured_dnf - computed_dnf) * 100.0;
    let percentage_diff = diff / configured_dnf;
    let should_notify = (percentage_diff.is_sign_positive()
        && percentage_diff <= dnf_increase_threshold)
        || (percentage_diff.is_sign_negative() && percentage_diff.abs() <= dnf_decrease_threshold);
    let should_notify = !should_notify;
    DnfNotify {
        configured_dnf,
        computed_dnf,
        percentage_diff,
        should_notify,
    }
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
            let dnf_notify = compute_dnf_notify(
                dnf,
                configured_dnf,
                serve_opt.dnf_increase_threshold,
                serve_opt.dnf_decrease_threshold,
            );
            tracing::info!(
                "Percentage DNF deviation for {market_id}: {} %",
                dnf_notify.percentage_diff
            );
            if dnf_notify.should_notify {
                tracing::info!("Going to send Slack notification");
                let (icon, status) = if dnf_notify.percentage_diff.is_sign_positive() {
                    (":chart_with_upwards_trend:", "increase")
                } else {
                    (":chart_with_downwards_trend:", "decrease")
                };
                let percentage_diff = dnf_notify.percentage_diff.abs().round();
                http_app
                    .send_notification(
                        format!("{icon} Detected DNF change for {market_id}"),
                        format!(
                            "Deviation {status}: *{percentage_diff}%* \n Recommended DNF: *{}*",
                            dnf_notify.computed_dnf.round()
                        ),
                    )
                    .await?;
            }
            tracing::info!(
                "Finished computing DNF for {market_id:?}: {} (Configured DNF: {})",
                dnf_notify.computed_dnf,
                dnf_notify.configured_dnf
            );
            app.market_params
                .write()
                .insert(market_id.clone(), dnf_notify);
        }
        tracing::info!("Going to sleep 24 hours");
        tokio::time::sleep(Duration::from_secs(60 * 60 * 24)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::coingecko::{CmcMarketPair, ExchangeId};

    #[test]
    fn sample_dnf_computation() {
        let exchanges = vec![
            CmcMarketPair {
                exchange_id: crate::coingecko::ExchangeId(50),
                exchange_name: "mexc".to_owned(),
                market_id: "LVN_USD".to_owned(),
                depth_usd_negative_two: 5828.0,
                depth_usd_positive_two: 7719.0,
                volume_24h_usd: 27304.39,
                outlier_detected: 0.2,
            },
            CmcMarketPair {
                exchange_id: ExchangeId(42),
                exchange_name: "gate.io".to_owned(),
                market_id: "LVN_USD".to_owned(),
                depth_usd_negative_two: 1756.0,
                depth_usd_positive_two: 22140.0,
                volume_24h_usd: 23065.95,
                outlier_detected: 0.0,
            },
        ];
        let dnf = super::compute_dnf_sensitivity(exchanges).unwrap();
        assert_eq!(dnf.0.round(), 268783.0, "Expected DNF");
    }

    #[test]
    fn validate_for_dnf_change_which_exceeds_threshold() {
        let dnf_notify = compute_dnf_notify(0.4, 1.0, 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff, 60.0);
        assert!(dnf_notify.should_notify);

        let dnf_notify = compute_dnf_notify(1.2, 1.0, 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff.round(), -20.0);
        assert!(dnf_notify.should_notify);
    }

    #[test]
    fn validate_for_dnf_change_for_happy_case() {
        let dnf_notify = compute_dnf_notify(0.8, 1.0, 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff.round(), 20.0);
        assert!(!dnf_notify.should_notify);

        let dnf_notify = compute_dnf_notify(1.05, 1.0, 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff.round(), -5.0);
        assert!(!dnf_notify.should_notify);

        let dnf_notify = compute_dnf_notify(1.0, 1.0, 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff, 0.0);
        assert!(!dnf_notify.should_notify);
    }
}
