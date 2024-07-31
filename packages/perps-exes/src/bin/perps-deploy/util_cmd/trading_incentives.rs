use std::collections::HashMap;
use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::Result;
use chrono::{DateTime, Datelike, Utc, Weekday};
use cosmos::{Address, AddressHrp};
use cosmwasm_std::Decimal256;
use perps_exes::config::{MainnetFactories, MainnetFactory};
use perps_exes::PerpsNetwork;
use reqwest::Url;
use shared::storage::{LvnToken, UnsignedDecimal, Usd};

#[derive(clap::Parser)]
pub(super) struct DistributionsCsvOpt {
    /// Directory path to contain intermediate csv files
    #[clap(
        long,
        env = "LEVANA_DISTRIBUTIONS_CACHE_DIR",
        default_value = ".cache/trading-incentives"
    )]
    pub(crate) cache_dir: PathBuf,
    /// Directory containing all trading incentives reports
    #[clap(long, default_value = "data/rewards")]
    pub(crate) rewards_dir: PathBuf,
    /// File name of the result csv file
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_FILENAME")]
    pub(crate) output: Option<PathBuf>,
    /// Start date of analysis period
    ///
    /// If omitted, uses the Sunday of the previous week
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_START_DATE")]
    pub(crate) start_date: Option<DateTime<Utc>>,
    /// End date of analysis period, defaults to 7 days after start date
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_END_DATE")]
    pub(crate) end_date: Option<DateTime<Utc>>,
    /// Minimum amount of LVN to distribution as rewards
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_MIN_REWARDS", default_value = "10")]
    pub(crate) min_rewards: LvnToken,
    /// Factory identifier
    #[clap(
        long,
        default_value = "osmomainnet1,ntrnmainnet1",
        value_delimiter = ','
    )]
    factories: Vec<String>,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: u32,
    /// Number of retries when an error occurs while generating a csv file
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_RETRIES", default_value_t = 3)]
    retries: u32,
    /// Size of the losses pool
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_LOSSES_POOL_SIZE")]
    losses_pool_size: LvnToken,
    /// Size of the fees pool
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_FEES_POOL_SIZE")]
    fees_pool_size: LvnToken,
    /// Percentage of referee rewards
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_REFEREE_REWARDS_PERCENTAGE")]
    referee_rewards_percentage: u64,
    /// Maximum cumulative referee rewards in USD
    #[clap(long, default_value = "1000")]
    max_referee_rewards: Usd,
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
        rewards_dir,
        output,
        start_date,
        end_date,
        min_rewards,
        factories,
        workers,
        retries,
        losses_pool_size,
        fees_pool_size,
        referee_rewards_percentage,
        cosmos_grpc_fallbacks,
        vesting_date,
        skip_data_load,
        max_referee_rewards,
    }: DistributionsCsvOpt,
    opt: Opt,
) -> Result<()> {
    let mainnet_factories = MainnetFactories::load()?;
    struct Factory<'a> {
        cache_file: PathBuf,
        name: String,
        factory: &'a MainnetFactory,
    }
    let factories = factories
        .into_iter()
        .map(|name| {
            let cache_file = cache_dir.join(format!("{name}.csv"));
            let factory = mainnet_factories.get(&name)?;
            tracing::info!("CSV filename: {}", cache_file.display());
            anyhow::Ok(Factory {
                cache_file,
                name,
                factory,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if !skip_data_load {
        for factory in &factories {
            if let Some(parent) = factory.cache_file.parent() {
                fs_err::create_dir_all(parent)?;
            }

            let mut attempted_retries = 0;
            while let Err(e) = open_position_csv(
                opt.clone(),
                OpenPositionCsvOpt {
                    factory: factory.name.clone(),
                    csv: factory.cache_file.clone(),
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
    }

    let start_date = start_date.unwrap_or_else(|| {
        let mut start_date = (Utc::now() - chrono::Duration::days(7)).date_naive();
        while start_date.weekday() != Weekday::Sun {
            start_date -= chrono::Duration::days(1);
        }

        start_date
            .and_hms_opt(0, 0, 0)
            .expect("Error adding hours/minutes/seconds")
            .and_utc()
    });

    let end_date = end_date.unwrap_or_else(|| start_date + chrono::Duration::days(7));
    let vesting_date = vesting_date.unwrap_or_else(|| end_date + chrono::Duration::days(180));

    let mut losses = TotalsTracker::default();
    let mut fees = TotalsTracker::default();
    let mut referee_fees = TotalsTracker::default();
    let client = reqwest::Client::new();

    for factory in factories {
        let mut is_referee_cache = HashMap::<Address, bool>::new();
        for record in csv::Reader::from_path(&factory.cache_file)?.into_deserialize() {
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
                losses.add(owner, pnl_usd.abs_unsigned())?;
            }
            fees.add(owner, trading_fee_usd)?;
            let is_referee = match is_referee_cache.get(&owner) {
                Some(x) => *x,
                None => {
                    let is_referee = validate_referee_wallet(
                        owner,
                        factory.factory.network,
                        factory.factory.address,
                        &client,
                    )
                    .await?;
                    is_referee_cache.insert(owner, is_referee);
                    is_referee
                }
            };
            if is_referee {
                referee_fees.add(owner, trading_fee_usd)?;
            }
        }
    }

    enum Category {
        Losses,
        Fees,
    }

    let output = output.unwrap_or_else(|| {
        let mut path = rewards_dir.clone();
        path.push(format!(
            "{}-trading-incentives.csv",
            start_date.date_naive()
        ));
        path
    });
    let output_canonical = if output.exists() {
        Some(output.canonicalize()?)
    } else {
        None
    };

    // Load up all previous rewards info so we can ensure we don't overpay anyone.
    let mut prev_referee_rewards = HashMap::<Address, Usd>::new();
    for file in fs_err::read_dir(&rewards_dir)? {
        let file = file?;
        let file = file.path().canonicalize()?;
        if Some(&file) == output_canonical.as_ref() {
            println!(
                "Skipping previous rewards for file we're generating now: {}",
                file.display()
            );
            continue;
        }
        for record in ::csv::Reader::from_path(&file)?.into_deserialize() {
            let record: DistributionsRecord = record?;
            if !record.referee_rewards_usd.is_zero() {
                let entry = prev_referee_rewards.entry(record.recipient).or_default();
                *entry = entry.checked_add(record.referee_rewards_usd)?;
            }
        }
    }

    let mut output = ::csv::Writer::from_path(&output)?;

    for (cat, pool_size, TotalsTracker { total, entries }) in [
        (Category::Losses, losses_pool_size, losses),
        (Category::Fees, fees_pool_size, fees),
    ] {
        for (recipient, amount) in entries {
            let amount = LvnToken::from_decimal256(
                amount.into_decimal256() * pool_size.into_decimal256() / total.into_decimal256(),
            );
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
                    Category::Losses => RewardType::Losses,
                    Category::Fees => RewardType::Fees,
                },
                Usd::zero(),
            )?;
        }
    }

    let TokenPriceResp { price, .. } = client
        .get("https://querier-mainnet.levana.finance/v1/levana/token-price")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let price = LvnPrice(price);
    for (recipient, amount) in referee_fees.entries {
        let refund_usd_uncapped = Usd::from_decimal256(
            amount.into_decimal256() * Decimal256::percent(referee_rewards_percentage),
        );
        let available_refund = match prev_referee_rewards.get(&recipient) {
            None => max_referee_rewards,
            Some(prev_amount) => {
                if prev_amount < &max_referee_rewards {
                    (max_referee_rewards - *prev_amount)?
                } else {
                    Usd::zero()
                }
            }
        };
        let refund_usd = refund_usd_uncapped.min(available_refund);
        let amount = price.usd_to_lvn(refund_usd);
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
            RewardType::Referee,
            refund_usd,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn serialize_record(
    output: &mut csv::Writer<std::fs::File>,
    min_rewards: LvnToken,
    recipient: Address,
    amount: LvnToken,
    title: String,
    vesting_date: DateTime<Utc>,
    r#type: RewardType,
    referee_rewards_usd: Usd,
) -> anyhow::Result<()> {
    if amount >= min_rewards {
        output.serialize(&DistributionsRecord {
            recipient: recipient.raw().with_hrp(AddressHrp::from_static("osmo")),
            amount,
            clawback: None,
            can_vote: false,
            can_receive_rewards: false,
            title,
            vesting_date,
            r#type,
            referee_rewards_usd,
            original_address: Some(recipient),
        })?;
    }
    Ok(())
}

async fn validate_referee_wallet(
    wallet: Address,
    network: PerpsNetwork,
    factory: Address,
    client: &reqwest::Client,
) -> Result<bool> {
    let url = reqwest::Url::parse_with_params(
        "https://querier-mainnet.levana.finance/v1/perps/referral-stats",
        &[
            ("network", &network.to_string()),
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
    total: Usd,
    entries: HashMap<Address, Usd>,
}

impl TotalsTracker {
    fn add(&mut self, wallet: Address, amount: Usd) -> Result<()> {
        self.total = self.total.checked_add(amount)?;
        let entry = self.entries.entry(wallet).or_default();
        *entry = entry.checked_add(amount)?;
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DistributionsRecord {
    pub(crate) recipient: Address,
    pub(crate) amount: LvnToken,
    pub(crate) vesting_date: DateTime<Utc>,
    pub(crate) clawback: Option<String>,
    pub(crate) can_vote: bool,
    pub(crate) can_receive_rewards: bool,
    pub(crate) title: String,
    pub(crate) r#type: RewardType,
    #[serde(default)]
    pub(crate) referee_rewards_usd: Usd,
    pub(crate) original_address: Option<Address>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RewardType {
    Losses,
    Fees,
    Referee,
}

#[derive(serde::Deserialize)]
struct TokenPriceResp {
    price: Decimal256,
}

#[derive(serde::Deserialize)]
struct ReferralStatsResp {
    referrer: Option<Address>,
}

/// Given as USD per LVN
struct LvnPrice(Decimal256);
impl LvnPrice {
    fn usd_to_lvn(&self, usd: Usd) -> LvnToken {
        LvnToken::from_decimal256(usd.into_decimal256() / self.0)
    }
}
