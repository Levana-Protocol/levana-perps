use std::collections::HashMap;
use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::Result;
use chrono::{DateTime, Utc};
use cosmos::Address;
use cosmwasm_std::Decimal256;
use perps_exes::config::MainnetFactories;
use reqwest::Url;
use shared::storage::UnsignedDecimal;

#[derive(clap::Parser)]
pub(super) struct DistributionsCsvOpt {
    /// Directory path to contain intermediate csv files
    #[clap(
        long,
        env = "LEVANA_DISTRIBUTIONS_CACHE_DIR",
        default_value = ".cache/trading-incentives"
    )]
    pub(crate) cache_dir: PathBuf,
    /// File name of the result csv file
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_FILENAME")]
    pub(crate) output: PathBuf,
    /// Start date of analysis period
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_START_DATE")]
    pub(crate) start_date: DateTime<Utc>,
    /// End date of analysis period, defaults to 7 days after start date
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_END_DATE")]
    pub(crate) end_date: Option<DateTime<Utc>>,
    /// Minimum amount of LVN to distribution as rewards
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_MIN_REWARDS", default_value = "10")]
    pub(crate) min_rewards: Decimal256,
    /// Factory identifier
    #[clap(long, default_value = "osmomainnet1")]
    factory: String,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: u32,
    /// Number of retries when an error occurs while generating a csv file
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_RETRIES", default_value_t = 3)]
    retries: u32,
    /// Size of the losses pool
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_LOSSES_POOL_SIZE")]
    losses_pool_size: Decimal256,
    /// Size of the fees pool
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_FEES_POOL_SIZE")]
    fees_pool_size: Decimal256,
    /// Percentage of referee rewards
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_REFEREE_REWARDS_PERCENTAGE")]
    referee_rewards_percentage: u64,
    /// Provide optional gRPC fallbacks URLs for factory
    #[clap(long, env = "COSMOS_GRPC_FALLBACKS", value_delimiter = ',')]
    cosmos_grpc_fallbacks: Vec<Url>,
    /// Vesting date, defaults to 180 days after end date.
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_VESTING_DATE")]
    vesting_date: Option<DateTime<Utc>>,
    /// Skip loading data from the chain, just use local cache
    #[clap(long)]
    skip_data_load: bool,
}

impl DistributionsCsvOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        distributions_csv(self, opt).await
    }
}

async fn distributions_csv(
    DistributionsCsvOpt {
        cache_dir,
        output,
        start_date,
        end_date,
        min_rewards,
        factory,
        workers,
        retries,
        losses_pool_size,
        fees_pool_size,
        referee_rewards_percentage,
        cosmos_grpc_fallbacks,
        vesting_date,
        skip_data_load,
    }: DistributionsCsvOpt,
    opt: Opt,
) -> Result<()> {
    let csv_filename: PathBuf = cache_dir.join(format!("{}.csv", factory.clone()));
    tracing::info!("CSV filename: {}", csv_filename.as_path().display());

    if let Some(parent) = csv_filename.parent() {
        fs_err::create_dir_all(parent)?;
    }

    if !skip_data_load {
        let mut attempted_retries = 0;
        while let Err(e) = open_position_csv(
            opt.clone(),
            OpenPositionCsvOpt {
                factory: factory.clone(),
                csv: csv_filename.clone(),
                workers,
                factory_primary_grpc: opt.cosmos_grpc.clone(),
                factory_fallbacks_grpc: cosmos_grpc_fallbacks.clone(),
            },
        )
        .await
        {
            if attempted_retries < retries {
                attempted_retries += 1;
                tracing::error!("Received error while generating csv files: {e}");
                tracing::info!("Retrying... Attempt {attempted_retries}/{retries}");
            } else {
                return Err(e);
            }
        }
    }

    let end_date = end_date.unwrap_or_else(|| start_date + chrono::Duration::days(7));
    let vesting_date = vesting_date.unwrap_or_else(|| end_date + chrono::Duration::days(180));

    let mut losses = TotalsTracker::default();
    let mut fees = TotalsTracker::default();

    for record in csv::Reader::from_path(&csv_filename)?.into_deserialize() {
        let PositionRecord {
            closed_at,
            owner,
            pnl_usd,
            trading_fee_usd,
            ..
        } = record?;
        let closed_at = match closed_at {
            Some(closed_at) => closed_at,
            None => continue,
        };
        if closed_at < start_date || closed_at >= end_date {
            continue;
        }
        if pnl_usd.is_negative() {
            losses.add(owner, pnl_usd.abs_unsigned().into_decimal256());
        }
        fees.add(owner, trading_fee_usd.into_decimal256());
    }

    enum Category {
        Losses,
        Fees,
    }

    tracing::info!("Writing distribution data to {}", output.display());
    let mut output = ::csv::Writer::from_path(&output)?;

    for (cat, pool_size, TotalsTracker { total, entries }) in [
        (Category::Losses, losses_pool_size, losses),
        (Category::Fees, fees_pool_size, fees.clone()),
    ] {
        for (recipient, amount) in entries {
            let amount = amount * pool_size / total;
            serialize_record(
                &mut output,
                min_rewards,
                recipient,
                amount,
                format!(
                    "Levana’s \"{}\" campaign, {} through {}",
                    match cat {
                        Category::Losses => "degens win",
                        Category::Fees => "trading incentives",
                    },
                    start_date.format("%Y-%m-%d"),
                    end_date.format("%Y-%m-%d")
                ),
                vesting_date,
                match cat {
                    Category::Losses => "losses",
                    Category::Fees => "fees",
                },
            )?;
        }
    }

    let client = reqwest::Client::new();
    let TokenPriceResp { price, .. } = client
        .get("https://querier-mainnet.levana.finance/v1/levana/token-price")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let network = factory.network.to_string();
    let factory = factory.address;
    for (recipient, amount) in fees.entries {
        if validate_referee_wallet(recipient, network.clone(), factory, client.clone()).await? {
            let amount = amount * Decimal256::percent(referee_rewards_percentage) / price;
            serialize_record(
                &mut output,
                min_rewards,
                recipient,
                amount,
                format!(
                    "Levana's \"referee rewards\" campaign, {} through {}",
                    start_date.format("%Y-%m-%d"),
                    end_date.format("%Y-%m-%d")
                ),
                vesting_date,
                "referee rewards",
            )?;
        }
    }

    Ok(())
}

fn serialize_record(
    output: &mut csv::Writer<std::fs::File>,
    min_rewards: Decimal256,
    recipient: Address,
    amount: Decimal256,
    title: String,
    vesting_date: DateTime<Utc>,
    r#type: &'static str,
) -> anyhow::Result<()> {
    if amount >= min_rewards {
        output.serialize(&DistributionsRecord {
            recipient,
            amount,
            clawback: None,
            can_vote: false,
            can_receive_rewards: false,
            title,
            vesting_date,
            r#type,
        })?;
    }
    Ok(())
}

async fn validate_referee_wallet(
    wallet: Address,
    network: String,
    factory: Address,
    client: reqwest::Client,
) -> Result<bool> {
    let url = reqwest::Url::parse_with_params(
        "https://querier-mainnet.levana.finance/v1/perps/referral-stats",
        &[
            ("network", &network),
            ("factory", &factory.to_string()),
            ("wallet", &wallet.to_string()),
        ],
    )?;
    let ReferralStatsResp { referrer } = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(referrer.is_some())
}

#[derive(Default, Clone)]
struct TotalsTracker {
    total: Decimal256,
    entries: HashMap<Address, Decimal256>,
}

impl TotalsTracker {
    fn add(&mut self, wallet: Address, amount: Decimal256) {
        self.total += amount;
        *self.entries.entry(wallet).or_default() += amount;
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DistributionsRecord {
    pub(crate) recipient: Address,
    pub(crate) amount: Decimal256,
    pub(crate) vesting_date: DateTime<Utc>,
    pub(crate) clawback: Option<String>,
    pub(crate) can_vote: bool,
    pub(crate) can_receive_rewards: bool,
    pub(crate) title: String,
    pub(crate) r#type: &'static str,
}

#[derive(serde::Deserialize)]
struct TokenPriceResp {
    price: Decimal256,
}

#[derive(serde::Deserialize)]
struct ReferralStatsResp {
    referrer: Option<Address>,
}
