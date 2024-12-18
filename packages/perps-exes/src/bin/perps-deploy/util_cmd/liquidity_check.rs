use anyhow::{anyhow, Context, Result};
use cosmwasm_std::Decimal256;
use perps_exes::contracts::MarketContract;
use perps_exes::{config::MainnetFactories, contracts::Factory};
use perpswap::number::{UnsignedDecimal, Usd};
use perpswap::storage::MarketId;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

use crate::cli::Opt;

#[derive(clap::Parser)]
pub(super) struct LiquidityCheckOpt {
    /// Slack webhook to publish the notification
    #[clap(long, env = "LEVANA_LIQUIDITY_CHECK_SLACK_WEBHOOK")]
    pub(crate) slack_webhook: reqwest::Url,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: u32,
    /// The threshold amount for the unlocked liquidity before sending an alert
    #[clap(
        long,
        default_value = "10",
        env = "LEVANA_LIQUIDITY_CHECK_UNLOCKED_LIQUIDITY_THRESHOLD_USD"
    )]
    unlocked_liquidity_threshold_usd: Usd,
    /// The percentage threshold for the unlocked liquidity compared to total liquidity
    #[clap(
        long,
        default_value = "10",
        env = "LEVANA_LIQUIDITY_CHECK_RATIO_THRESHOLD"
    )]
    ratio_threshold: Decimal256,
    /// Factory identifier
    #[clap(
        long,
        default_value = "osmomainnet1",
        env = "LEVANA_LIQUIDITY_CHECK_FACTORY"
    )]
    factory: String,
    /// Run check after specified seconds
    #[arg(
        long,
        env = "LEVANA_LIQUIDITY_RECALC_FREQ_SECONDS",
        default_value = "3600"
    )]
    pub(crate) recalculation_frequency_in_seconds: u64,
}

impl LiquidityCheckOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(self, opt).await
    }
}

#[derive(Clone)]
struct MarketInfo {
    market: MarketContract,
    market_id: Arc<MarketId>,
}

async fn go(
    LiquidityCheckOpt {
        slack_webhook,
        unlocked_liquidity_threshold_usd,
        ratio_threshold,
        workers,
        factory,
        recalculation_frequency_in_seconds,
    }: LiquidityCheckOpt,
    opt: Opt,
) -> Result<()> {
    let mainnet_factories = MainnetFactories::load()?;
    let factory = mainnet_factories.get(&factory)?;

    let cosmos = opt.load_app_mainnet(factory.network).await?.cosmos;
    let factory = Factory::from_contract(cosmos.make_contract(factory.address));

    loop {
        tracing::info!("Started Liquidity check for the markets.");
        let markets = factory.get_markets().await?;
        let market_count = markets.len();
        tracing::info!(
            "Fetched {} markets' information for Liquidity check.",
            market_count
        );

        let mut market_info = Vec::<MarketInfo>::new();

        for market in markets {
            let market_id = market.market_id.into();
            let market = MarketContract::new(market.market);
            market_info.push(MarketInfo { market, market_id })
        }

        let mut set = JoinSet::new();

        let market_count_per_worker = market_count.div_euclid(workers.try_into()?);
        let market_remainder = market_count.rem_euclid(workers.try_into()?);
        let mut start = 0;
        for worker_id in 0..workers.try_into()? {
            let extra = if worker_id < market_remainder { 1 } else { 0 };
            let end = start + market_count_per_worker + extra;
            set.spawn(liquidity_check_helper(
                market_info[start..end].to_vec(),
                unlocked_liquidity_threshold_usd,
                ratio_threshold,
            ));
            start = end;
        }

        let mut volatile_market_ids = Vec::new();

        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(ids)) => {
                    volatile_market_ids.extend(ids);
                }
                Ok(Err(e)) => {
                    set.abort_all();
                    return Err(e);
                }
                Err(e) => {
                    set.abort_all();
                    return Err(e).context("Unexpected panic");
                }
            }
        }

        if !volatile_market_ids.is_empty() {
            tracing::info!(
                "Found {} volatile markets, sending a slack notification.",
                volatile_market_ids.len()
            );
            send_slack_notification(
                slack_webhook.clone(),
                "Volatile markets found".to_owned(),
                format!("Markets: {:?}", volatile_market_ids),
            )
            .await?;
        }
        let duration = Duration::from_secs(recalculation_frequency_in_seconds);
        tracing::info!("Completed market Liquidity check, Going to sleep {duration:?}.");
        tokio::time::sleep(duration).await;
    }
}

async fn liquidity_check_helper(
    market_info: Vec<MarketInfo>,
    unlocked_liquidity_threshold_usd: Usd,
    ratio_threshold: Decimal256,
) -> Result<Vec<Arc<MarketId>>> {
    let mut volatile_market_ids = Vec::new();

    for target_market in market_info {
        let contract = target_market.market.clone();
        let market_id = target_market.market_id.clone();

        let status = contract.status().await?;

        let total_liquidity = status
            .liquidity
            .locked
            .checked_add(status.liquidity.unlocked)?;
        let price_point = contract.current_price().await?;

        if total_liquidity.is_zero()
            || status
                .liquidity
                .unlocked
                .into_decimal256()
                .checked_mul(Decimal256::from_ratio(100u32, 1u32))?
                .checked_div(total_liquidity.into_decimal256())?
                < ratio_threshold
            || price_point.collateral_to_usd(status.liquidity.unlocked)
                < unlocked_liquidity_threshold_usd
        {
            volatile_market_ids.push(market_id);
        }
    }
    Ok(volatile_market_ids)
}

pub(crate) async fn send_slack_notification(
    webhook: reqwest::Url,
    header: String,
    message: String,
) -> anyhow::Result<()> {
    let value = serde_json::json!(
    {
        "text": "volatile markets alert",
        "blocks": [
            {
                "type": "header",
                "text": {
                    "type": "plain_text",
                    "text": header,
                }
            },
            {
                "type": "section",
                "block_id": "section567",
                "text": {
                    "type": "mrkdwn",
                    "text": message,
                },
                "accessory": {
                    "type": "image",
                    "image_url": "https://static.levana.finance/icons/levana-token.png",
                    "alt_text": "Levana Dragons"
                }
            }
        ]
    });
    let client = Client::new();
    let response = client.post(webhook.clone()).json(&value).send().await?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(anyhow!(
            "Slack notification POST request failed with code {}",
            response.status()
        ))
    }
}
