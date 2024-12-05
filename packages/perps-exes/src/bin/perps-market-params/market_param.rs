use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, bail, ensure, Context};
use chrono::{DateTime, Days, NaiveDate, Utc};
use perpswap::storage::MarketId;

use crate::{
    cli::{Opt, ServeOpt},
    coingecko::{CmcMarketPair, ExchangeKind},
    s3::S3,
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
    pub(crate) max_leverage: String,
}

impl MarketsConfig {
    pub(crate) fn get_chain_dnf(&self, market_id: &MarketId) -> anyhow::Result<DnfInNotional> {
        let result = self
            .markets
            .iter()
            .find(|item| item.status.market_id == *market_id)
            .map(|item| item.status.config.delta_neutrality_fee_sensitivity.clone())
            .context("No dnf found")?;
        let result = result.parse()?;
        Ok(DnfInNotional(result))
    }

    pub(crate) fn get_chain_max_leverage(
        &self,
        market_id: &MarketId,
    ) -> anyhow::Result<MaxLeverage> {
        let result = self
            .markets
            .iter()
            .find(|item| item.status.market_id == *market_id)
            .map(|item| item.status.config.max_leverage.clone())
            .context("No max_leverage found")?;
        let leverage = result.parse()?;
        Ok(MaxLeverage::new(leverage))
    }
}

pub(crate) fn dnf_sensitivity_to_max_leverage(dnf_sensitivity: DnfInUsd) -> MaxLeverage {
    let dnf_sensitivity = dnf_sensitivity.0;
    let million = 1000000.0;
    let leverage = if dnf_sensitivity < (20.0 * million) {
        4.0
    } else if dnf_sensitivity >= (20.0 * million) && dnf_sensitivity < (50.0 * million) {
        10.0
    } else if dnf_sensitivity >= (50.0 * million) && dnf_sensitivity < (200.0 * million) {
        30.0
    } else {
        50.0
    };
    MaxLeverage::new(leverage)
}

#[derive(
    PartialOrd, PartialEq, Clone, serde::Serialize, serde::Deserialize, Copy, Default, Debug,
)]
pub(crate) struct DnfInNotional(pub(crate) f64);

impl Display for DnfInNotional {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

impl DnfInNotional {
    pub(crate) async fn as_asset_amount(
        &self,
        asset: NotionalAsset<'_>,
        http_app: &HttpApp,
    ) -> anyhow::Result<DnfInUsd> {
        if asset.is_usd_equiv() {
            Ok(DnfInUsd(self.0))
        } else {
            let price = http_app.get_price_in_usd(asset).await?;
            Ok(DnfInUsd(self.0 * price))
        }
    }
}

#[derive(PartialOrd, PartialEq, Clone, serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct DnfInUsd(pub(crate) f64);

impl Display for DnfInUsd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

#[derive(PartialEq, Clone, serde::Serialize, serde::Deserialize, Copy, Debug, Default)]
pub(crate) struct MinDepthLiquidity(pub(crate) f64);

impl Display for MinDepthLiquidity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl MinDepthLiquidity {
    pub(crate) fn new(num: f64) -> anyhow::Result<MinDepthLiquidity> {
        if num.is_nan() {
            Err(anyhow!("Invalid min depth liquidity"))
        } else {
            Ok(MinDepthLiquidity(num))
        }
    }
}

impl Eq for MinDepthLiquidity {}

impl Ord for MinDepthLiquidity {
    fn cmp(&self, other: &MinDepthLiquidity) -> Ordering {
        // We assume that it doesn't contain NAN as part of its
        // domain.
        self.0.partial_cmp(&other.0).unwrap()
    }
}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for MinDepthLiquidity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub(crate) struct Dnf {
    pub(crate) dnf_in_notional: DnfInNotional,
    pub(crate) dnf_in_usd: DnfInUsd,
    pub(crate) min_depth_liquidity: MinDepthLiquidity,
}

impl DnfInUsd {
    async fn to_asset_amount(
        &self,
        asset: NotionalAsset<'_>,
        http_app: &HttpApp,
    ) -> anyhow::Result<f64> {
        let price = http_app.get_price_in_usd(asset).await?;
        Ok(self.0 / price)
    }
}

// Compute DNF sensitivity of the current day.
pub(crate) async fn dnf_sensitivity(
    http_app: &HttpApp,
    market_id: &MarketId,
) -> anyhow::Result<Dnf> {
    tracing::debug!("Going to compute dnf_sensitivity for {market_id}");
    let base_asset = AssetName(market_id.get_base());
    let quote_asset = AssetName(market_id.get_quote());

    if quote_asset.is_usd_equiv() {
        tracing::debug!("Fetch exchanges");
        let exchanges = http_app.get_market_pair(base_asset).await?;
        tracing::debug!(
            "Total exchanges found: {} for {market_id:?}",
            exchanges.len()
        );
        let dnf_result = compute_dnf_sensitivity(exchanges)?;
        let dnf_in_usd = dnf_result.dnf;
        let dnf_in_base = dnf_in_usd
            .to_asset_amount(NotionalAsset(base_asset.0), http_app)
            .await?;
        return Ok(Dnf {
            dnf_in_notional: DnfInNotional(dnf_in_base),
            dnf_in_usd: DnfInUsd(dnf_in_usd.0),
            min_depth_liquidity: MinDepthLiquidity::new(dnf_result.min_depth_liquidity)?,
        });
    }

    let notional_asset = NotionalAsset(market_id.get_notional());

    tracing::debug!("Fetch base_exchanges");
    let base_exchanges = http_app.get_market_pair(base_asset).await?;
    tracing::debug!("Fetch quote_exchanges");
    let quote_exchanges = http_app.get_market_pair(quote_asset).await?;
    let base_dnf_in_usd = compute_dnf_sensitivity(base_exchanges)?;
    let quote_dnf_in_usd = compute_dnf_sensitivity(quote_exchanges)?;
    let dnf_in_usd = if base_dnf_in_usd > quote_dnf_in_usd {
        quote_dnf_in_usd
    } else {
        base_dnf_in_usd
    };
    let dnf_in_base = dnf_in_usd
        .dnf
        .to_asset_amount(notional_asset, http_app)
        .await?;
    Ok(Dnf {
        dnf_in_notional: DnfInNotional(dnf_in_base),
        dnf_in_usd: DnfInUsd(dnf_in_usd.dnf.0),
        min_depth_liquidity: MinDepthLiquidity::new(dnf_in_usd.min_depth_liquidity)?,
    })
}

#[derive(Clone, Copy)]
pub(crate) struct AssetName<'a>(pub(crate) &'a str);

#[derive(Clone, Copy, Debug)]
pub(crate) struct NotionalAsset<'a>(pub(crate) &'a str);

impl AssetName<'_> {
    /// Is the asset either USD or a stablecoin pinned to USD?
    fn is_usd_equiv(&self) -> bool {
        self.0 == "USD" || self.0 == "USDC" || self.0 == "USDT"
    }
}

impl NotionalAsset<'_> {
    /// Is the asset either USD or a stablecoin pinned to USD?
    fn is_usd_equiv(&self) -> bool {
        self.0 == "USD" || self.0 == "USDC" || self.0 == "USDT"
    }
}

struct DnfExchanges {
    exchanges: Vec<CmcMarketPair>,
    max_exchange: CmcMarketPair,
}

fn filter_invalid_exchanges(exchanges: Vec<CmcMarketPair>) -> anyhow::Result<DnfExchanges> {
    // Check if top tier exchange is present so that we can filter
    // accordingly.
    let has_top_tier = exchanges
        .clone()
        .iter()
        .any(|item| item.exchange_id.is_top_tier());

    let exchanges = exchanges.into_iter().filter(|exchange| {
        exchange.exchange_name.to_lowercase() != "htx" && exchange.outlier_detected < 0.3
    });

    let exchanges = exchanges
        .map(|exchange| {
            let exchange_type = exchange.exchange_id.exchange_type();
            exchange_type.map(|exchange_kind| (exchange, exchange_kind))
        })
        .collect::<anyhow::Result<Vec<_>>>();

    let exchanges = exchanges?
        .into_iter()
        .filter_map(|(exchange, exchange_type)| match exchange_type {
            ExchangeKind::Cex => Some(exchange),
            ExchangeKind::Dex => None,
        });

    let max_volume_exchange = if has_top_tier {
        exchanges
            .clone()
            .max_by(|a, b| {
                a.depth_usd_positive_two
                    .min(a.depth_usd_negative_two)
                    .total_cmp(&b.depth_usd_positive_two.min(b.depth_usd_negative_two))
            })
            .context("No max value found")?
    } else {
        // This condition is specifically for handling market like
        // MAGA_USDC which isn't supported by top tier markets.  CMC
        // API has outlier field which is useless in their API. It
        // doesn't flag an exchange like B2Z Exchange even though in
        // their UI it shows that it has low confidence on that
        // exchange.
        //
        // So for these cases, we use 24 hour volume data as that
        // seems to filter out these exchanges and instead choses a
        // exchange with a better confidence.
        exchanges
            .clone()
            .max_by(|a, b| a.volume_24h_usd.total_cmp(&b.volume_24h_usd))
            .context("No max value found")?
    };

    let should_filter_top_tier = if has_top_tier {
        !max_volume_exchange.exchange_id.is_top_tier()
    } else {
        false
    };

    if max_volume_exchange.depth_usd_negative_two == 0.0
        || max_volume_exchange.depth_usd_positive_two == 0.0
        || should_filter_top_tier
    {
        tracing::debug!(
            "Skipping exchange id {:?} with liqudity depth of {}",
            max_volume_exchange.exchange_id,
            max_volume_exchange.depth_usd_positive_two
        );
        // Skip this exchange
        let exchanges: Vec<_> = exchanges
            .into_iter()
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

#[derive(PartialOrd, PartialEq, Clone, serde::Serialize)]
pub(crate) struct DnfResult {
    pub(crate) dnf: DnfInUsd,
    pub(crate) min_depth_liquidity: f64,
}

fn compute_dnf_sensitivity(exchanges: Vec<CmcMarketPair>) -> anyhow::Result<DnfResult> {
    // Algorithm: https://staff.levana.finance/new-market-checklist#dnf-sensitivity
    tracing::debug!("Total exchanges: {}", exchanges.len());
    let dnf_exchange = filter_invalid_exchanges(exchanges)?;

    let exchanges = dnf_exchange.exchanges.iter();

    let max_volume_exchange = dnf_exchange.max_exchange;

    tracing::debug!("Max volume exchange: {max_volume_exchange:#?}");
    let total_volume_percentage = exchanges
        .clone()
        .map(|exchange| exchange.volume_24h_usd)
        .sum::<f64>();
    let market_share = max_volume_exchange.volume_24h_usd / total_volume_percentage;
    tracing::debug!(
        "Market share: {market_share}, Total exchanges considered: {}",
        exchanges.count()
    );
    let min_depth_liquidity = max_volume_exchange
        .depth_usd_negative_two
        .min(max_volume_exchange.depth_usd_positive_two);
    let dnf = (min_depth_liquidity / market_share) * 25.0;
    tracing::debug!("DNF: {dnf}");
    let result = DnfResult {
        dnf: DnfInUsd(dnf),
        min_depth_liquidity,
    };
    Ok(result)
}

#[derive(Debug, Clone, serde::Serialize, Copy, PartialEq, serde::Deserialize, PartialOrd)]
pub(crate) struct MaxLeverage(f64);

impl Display for MaxLeverage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl MaxLeverage {
    pub(crate) fn new(leverage: f64) -> Self {
        Self(leverage)
    }

    pub(crate) fn raw(&self) -> f64 {
        self.0
    }
}

#[derive(Clone, serde::Serialize, Debug)]
pub(crate) struct DnfNotify {
    pub(crate) configured_dnf: DnfInNotional,
    pub(crate) computed_dnf: DnfInNotional,
    pub(crate) percentage_diff: f64,
    pub(crate) should_notify: bool,
    pub(crate) status: ConfiguredDnfStatus,
}

#[derive(Clone, serde::Serialize, Debug)]
pub(crate) enum ConfiguredDnfStatus {
    // If configured_dnf - computed_dnf is positive, it means that the
    // configured_dnf should be modified to be lower.  And hence the
    // configured_dnf is lenient and should be lowered.
    Lenient,
    Strict,
}

pub(crate) fn compute_dnf_notify(
    comp @ DnfInNotional(computed_dnf): DnfInNotional,
    conf @ DnfInNotional(configured_dnf): DnfInNotional,
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
        configured_dnf: conf,
        computed_dnf: comp,
        percentage_diff,
        should_notify,
        status: if percentage_diff.is_sign_positive() {
            ConfiguredDnfStatus::Lenient
        } else {
            ConfiguredDnfStatus::Strict
        },
    }
}

fn get_market_file_path(market_id: &MarketId, data_dir: &Path) -> PathBuf {
    let filename = format!("{market_id}_data.json");
    data_dir.join(filename)
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct HistoricalData {
    pub(crate) data: Vec<DnfRecord>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct DnfRecord {
    pub(crate) date: NaiveDate,
    pub(crate) dnf: Dnf,
    pub(crate) max_leverage: MaxLeverage,
    pub(crate) last_updated: DateTime<Utc>,
}

impl HistoricalData {
    pub(crate) async fn save(
        &self,
        market_id: &MarketId,
        data_dir: PathBuf,
        until: Option<u16>,
        s3_client: &S3,
    ) -> anyhow::Result<()> {
        save_historical_data(market_id, data_dir, self.clone(), until, s3_client).await?;
        Ok(())
    }

    pub(crate) fn append(
        &mut self,
        dnf: Dnf,
        max_leverage: MaxLeverage,
        last_updated: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let now = last_updated.date_naive();
        let result = DnfRecord {
            date: now,
            dnf: dnf.clone(),
            max_leverage,
            last_updated,
        };
        let mut changed_historical_data = false;
        let mut had_today_data = false;
        let new_historical_data = self
            .data
            .clone()
            .into_iter()
            .map(|item| {
                if item.date == now {
                    had_today_data = true;
                    if dnf.min_depth_liquidity < item.dnf.min_depth_liquidity {
                        tracing::info!("Updating data since lower liquidity depth was found");
                        changed_historical_data = true;
                        result.clone()
                    } else {
                        item
                    }
                } else {
                    item
                }
            })
            .collect();
        if changed_historical_data {
            self.data = new_historical_data;
        } else if !had_today_data {
            self.data.push(result);
        }
        Ok(())
    }

    pub(crate) fn compute_max_leverage(
        &self,
        days_to_consider: u16,
    ) -> anyhow::Result<MaxLeverage> {
        let historical_data = self.till_days(Some(days_to_consider))?;
        let leverage = historical_data
            .data
            .iter()
            .min_by(|x, y| {
                x.max_leverage
                    .raw()
                    .partial_cmp(&y.max_leverage.raw())
                    .expect("max_leverage comparison failed")
            })
            .context("Historical data doesn't have a single max_leverage")?;
        Ok(leverage.max_leverage)
    }

    pub(crate) fn compute_dnf(&self, days_to_consider: u16) -> anyhow::Result<Dnf> {
        let mut historical_data = self.till_days(Some(days_to_consider))?;
        historical_data
            .data
            .sort_by_key(|item| item.dnf.min_depth_liquidity);
        let result = historical_data
            .data
            .into_iter()
            .next()
            .context("Empty historical data")?;
        Ok(result.dnf)
    }

    pub(crate) fn till_days(
        &self,
        days_to_consider: Option<u16>,
    ) -> anyhow::Result<HistoricalData> {
        let days_to_consider = match days_to_consider {
            Some(days) => days,
            None => {
                return Ok(HistoricalData {
                    data: self.data.clone(),
                })
            }
        };

        let data = self.data.clone().into_iter();
        let mut now = Utc::now().date_naive();
        let mut required_dates = vec![];
        for _ in 1..=days_to_consider {
            required_dates.push(now);
            now = now - Days::new(1);
        }
        let result: Vec<_> = data
            .filter(|item| required_dates.contains(&item.date))
            .collect();
        ensure!(
            result.len() == usize::from(days_to_consider),
            "Historical data ({}) is not matching the total days for calcuation",
            result.len()
        );
        Ok(HistoricalData { data: result })
    }
}

pub(crate) async fn load_historical_data(
    market_id: &MarketId,
    data_dir: PathBuf,
    s3_client: &S3,
) -> anyhow::Result<HistoricalData> {
    let path = get_market_file_path(market_id, &data_dir);
    let result = s3_client
        .download(&path)
        .await
        .context("Error downloading file from S3");
    match result {
        Ok(historical_data) => Ok(historical_data),
        Err(_) => Ok(HistoricalData { data: vec![] }),
    }
}

pub(crate) async fn save_historical_data(
    market_id: &MarketId,
    data_dir: PathBuf,
    data: HistoricalData,
    untill: Option<u16>,
    s3_client: &S3,
) -> anyhow::Result<()> {
    let path = get_market_file_path(market_id, &data_dir);
    let data = data.till_days(untill)?;
    s3_client
        .upload(&path, &data)
        .await
        .context("Error writing historical data to S3 Bucket")?;
    Ok(())
}

pub(crate) async fn compute_coin_dnfs(
    app: Arc<NotifyApp>,
    serve_opt: ServeOpt,
    opt: Opt,
) -> anyhow::Result<()> {
    let http_app = HttpApp::new(Some(serve_opt.slack_webhook.clone()), opt.cmc_key.clone());
    let data_dir = serve_opt.cmc_data_dir.clone();
    let s3_client = S3::new(serve_opt.bucket_id.clone())
        .await
        .context("Could not create AWS S3 client")?;
    let mut market_analysis_counter = 0;
    let mut last_notified_dates: HashMap<MarketId, NaiveDate> = HashMap::new();
    loop {
        tracing::info!("Going to fetch market status from querier");
        market_analysis_counter += 1;
        let market_config = http_app
            .fetch_market_status(&serve_opt.mainnet_factories[..])
            .await?;
        let markets = market_config
            .clone()
            .markets
            .into_iter()
            .filter(|market| !serve_opt.skip_market_ids.contains(&market.status.market_id))
            .collect::<Vec<_>>();
        let mut error_markets = vec![];

        for market_id in &markets {
            let market_id = &market_id.status.market_id;
            app.markets.write().insert(market_id.clone());
            tracing::info!("Going to compute DNF for {market_id:?}");
            let mut historical_data =
                load_historical_data(market_id, data_dir.clone(), &s3_client).await?;
            tracing::info!(
                "Fetched  historical data for {market_id}: {}",
                historical_data.data.len()
            );
            // Compute todays data and save it
            let now = Utc::now();
            let dnf = dnf_sensitivity(&http_app, market_id).await;
            let (dnf, max_leverage) = {
                let dnf = match dnf {
                    Ok(dnf) => dnf,
                    Err(ref error) => {
                        if error.to_string().contains("Exchange type not known for id") {
                            error_markets.push(market_id);
                            continue;
                        } else {
                            dnf?
                        }
                    }
                };
                let max_leverage = dnf_sensitivity_to_max_leverage(dnf.dnf_in_usd.clone());
                tracing::debug!(
                    "DNF Notional {}, Max leverage: {max_leverage}, Notional_asset: {}",
                    dnf.dnf_in_notional,
                    market_id.get_notional()
                );
                (dnf, max_leverage)
            };
            historical_data.append(dnf, max_leverage, now)?;
            let new_historical_data = historical_data.till_days(Some(serve_opt.cmc_data_age_days));
            let until_days = match new_historical_data.is_ok() {
                true => Some(serve_opt.cmc_data_age_days),
                false => None,
            };
            if (market_analysis_counter >= serve_opt.required_runs_slack_alert)
                && new_historical_data.is_ok()
                && Some(&now.date_naive()) != last_notified_dates.get(market_id)
            {
                tracing::info!("Computing DNF using historical data");
                let historical_market_dnf =
                    historical_data.compute_dnf(serve_opt.cmc_data_age_days)?;
                let historical_max_leverage =
                    historical_data.compute_max_leverage(serve_opt.cmc_data_age_days)?;
                let market_status = check_market_status(
                    &market_config,
                    market_id,
                    &http_app,
                    &historical_market_dnf,
                    serve_opt.clone(),
                    historical_max_leverage,
                )
                .await?;
                app.market_params
                    .write()
                    .insert(market_id.clone(), market_status);
                historical_data = new_historical_data?;

                let entry = last_notified_dates.entry(market_id.to_owned()).or_default();
                *entry = now.date_naive();
            }
            historical_data
                .save(market_id, data_dir.clone(), until_days, &s3_client)
                .await?;
            if serve_opt.cmc_wait_seconds > 0 {
                tracing::info!(
                    "Going to sleep {} seconds to avoid getting rate limited",
                    serve_opt.cmc_wait_seconds
                );
                tokio::time::sleep(Duration::from_secs(serve_opt.cmc_wait_seconds)).await;
            }
        }
        if !error_markets.is_empty() {
            let description = format!("Markets: {:?}", error_markets);
            http_app
                .send_notification("MPA: Unrecognized exchanges found".to_owned(), description)
                .await?;
        }

        market_analysis_counter %= serve_opt.required_runs_slack_alert;
        let duration = Duration::from_secs(serve_opt.recalcuation_frequency_in_seconds);
        tracing::info!("Completed market analysis (Round: {market_analysis_counter}), Going to sleep {duration:?}");
        tokio::time::sleep(duration).await;
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct MarketStatusResult {
    pub(crate) dnf_notify: DnfNotify,
    pub(crate) historical_max_leverage: MaxLeverage,
    pub(crate) configured_max_leverage: MaxLeverage,
}

async fn check_market_status(
    market_config: &MarketsConfig,
    market_id: &MarketId,
    http_app: &HttpApp,
    historical_market_dnf: &Dnf,
    serve_opt: ServeOpt,
    historical_max_leverage: MaxLeverage,
) -> anyhow::Result<MarketStatusResult> {
    tracing::info!("Checking market status for {market_id}");
    let configured_dnf = market_config
        .get_chain_dnf(market_id)
        .context(format!("No DNF configured for {market_id:?}"))?;
    let configured_max_leverage = market_config
        .get_chain_max_leverage(market_id)
        .context(format!("No max_leverage configured for {market_id:?}"))?;

    tracing::info!("Configured max_leverage for {market_id}: {configured_max_leverage:?}");
    tracing::info!(
        "Recommended max_leverage based on historical data for {market_id}: {historical_max_leverage:?}"
    );

    if configured_max_leverage != historical_max_leverage {
        http_app
            .send_notification(
                format!(":information_source: Recommended Max leverage change for {market_id}"),
                format!(
                    "Configured Max leverage: *{configured_max_leverage:?}* \n Recommended Max leverage: *{historical_max_leverage:?}*",

                ),
            )
            .await?;
    }

    let dnf_notify = compute_dnf_notify(
        historical_market_dnf.dnf_in_notional,
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
        let percentage_diff = dnf_notify.percentage_diff.abs().round();
        let (icon, status) = match dnf_notify.status {
            ConfiguredDnfStatus::Lenient => (
                ":chart_with_downwards_trend:",
                format!("lenient (*Decrease* it by {}%)", percentage_diff),
            ),
            ConfiguredDnfStatus::Strict => (
                ":chart_with_upwards_trend:",
                format!("strict (*Increase* it by {}%)", percentage_diff),
            ),
        };

        http_app
            .send_notification(
                format!("{icon} Recommended DNF change for {market_id}"),
                format!(
                    "Configured DNF is {status} \n Configured DNF: *{}* \n Recommended DNF: *{}*",
                    dnf_notify.configured_dnf.0,
                    dnf_notify.computed_dnf.0.round()
                ),
            )
            .await?;
    }
    tracing::info!(
        "Finished computing DNF for {market_id:?}: {} (Configured DNF: {})",
        dnf_notify.computed_dnf.0,
        dnf_notify.configured_dnf.0
    );
    let result = MarketStatusResult {
        dnf_notify,
        historical_max_leverage,
        configured_max_leverage,
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_for_dnf_change_which_exceeds_threshold() {
        let dnf_notify = compute_dnf_notify(DnfInNotional(0.4), DnfInNotional(1.0), 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff, 60.0);
        assert!(dnf_notify.should_notify);

        let dnf_notify = compute_dnf_notify(DnfInNotional(1.2), DnfInNotional(1.0), 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff.round(), -20.0);
        assert!(dnf_notify.should_notify);
    }

    #[test]
    fn validate_for_dnf_change_for_happy_case() {
        let dnf_notify = compute_dnf_notify(DnfInNotional(0.8), DnfInNotional(1.0), 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff.round(), 20.0);
        assert!(!dnf_notify.should_notify);

        let dnf_notify = compute_dnf_notify(DnfInNotional(1.05), DnfInNotional(1.0), 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff.round(), -5.0);
        assert!(!dnf_notify.should_notify);

        let dnf_notify = compute_dnf_notify(DnfInNotional(1.0), DnfInNotional(1.0), 50.0, 10.0);
        assert_eq!(dnf_notify.percentage_diff, 0.0);
        assert!(!dnf_notify.should_notify);
    }

    #[test]
    fn test_min_depth_sort() {
        let mut data = [
            MinDepthLiquidity(1.0),
            MinDepthLiquidity(9.0),
            MinDepthLiquidity(4.0),
        ];
        data.sort();
        let last = data.last().unwrap();
        assert_eq!(*last, MinDepthLiquidity(9.0));
    }

    #[test]
    fn max_leverage_test() {
        let result = Dnf::default();
        let date = Utc::now().date_naive();
        let data = HistoricalData {
            data: vec![
                DnfRecord {
                    date,
                    dnf: result.clone(),
                    max_leverage: MaxLeverage::new(2.0),
                    last_updated: Utc::now(),
                },
                DnfRecord {
                    date: date.checked_sub_days(Days::new(1)).unwrap(),
                    dnf: result.clone(),
                    max_leverage: MaxLeverage::new(1.0),
                    last_updated: Utc::now(),
                },
                DnfRecord {
                    date: date.checked_sub_days(Days::new(5)).unwrap(),
                    dnf: result.clone(),
                    max_leverage: MaxLeverage::new(-5.0),
                    last_updated: Utc::now(),
                },
            ],
        };
        let max_leverage = data.compute_max_leverage(2).unwrap();
        assert_eq!(max_leverage.raw(), 1.0);
    }
}
