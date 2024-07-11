use std::collections::HashMap;
use std::ops::{Add, Div, Mul};
use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{load_data_from_csv, open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cosmos::Address;
use cosmwasm_std::Decimal256;
use itertools::Itertools;
use reqwest::Url;
use shared::storage::{UnsignedDecimal, Usd};

#[derive(clap::Parser)]
pub(super) struct DistributionsCsvOpt {
    /// Directory path to contain csv files
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_BUFF_DIR")]
    pub(crate) buff_dir: PathBuf,
    /// File name of the result csv file
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_FILENAME")]
    pub(crate) filename: String,
    /// Start date of analysis period
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_START_DATE")]
    pub(crate) start_date: DateTime<Utc>,
    /// End date of analysis period
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_END_DATE")]
    pub(crate) end_date: DateTime<Utc>,
    /// Rounding threshold for distributions data
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_THRESHOLD")]
    pub(crate) threshold: Decimal256,
    /// Factory identifier
    #[clap(long)]
    factory: String,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: u32,
    /// Number of retries when an error occurs while generating a csv file
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_RETRIES", default_value_t = 3)]
    retries: u32,
    /// Size of the losses pool
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_LOSSES_POOL_SIZE")]
    losses_pool_size: u32,
    /// Size of the fees pool
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_FEES_POOL_SIZE")]
    fees_pool_size: u32,
    /// Provide optional gRPC fallbacks URLs for factory
    #[clap(long, env = "COSMOS_GRPC_FALLBACKS", value_delimiter = ',')]
    cosmos_grpc_fallbacks: Vec<Url>,
}

impl DistributionsCsvOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        distributions_csv(self, opt).await
    }
}

async fn distributions_csv(
    DistributionsCsvOpt {
        buff_dir,
        filename,
        start_date,
        end_date,
        threshold,
        factory,
        workers,
        retries,
        losses_pool_size,
        fees_pool_size,
        cosmos_grpc_fallbacks,
    }: DistributionsCsvOpt,
    opt: Opt,
) -> Result<()> {
    let csv_filename: PathBuf = buff_dir.join(format!("{}.csv", factory.clone()));
    tracing::info!("CSV filename: {}", csv_filename.as_path().display());

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

    tracing::info!("Reading csv data");
    let csv_data = load_data_from_csv(&csv_filename).with_context(|| {
        format!(
            "Unable to load old CSV data from {}",
            csv_filename.display()
        )
    })?;

    let distributions_data = generate_distributions_data(
        csv_data.values().collect_vec(),
        losses_pool_size,
        fees_pool_size,
        start_date,
        end_date,
        threshold,
    )?;

    tracing::info!("Writing distribution data to {filename}");
    let mut csv = ::csv::Writer::from_path(filename)?;
    for record in distributions_data.into_iter() {
        csv.serialize(&record)?;
        csv.flush()?;
    }

    Ok(())
}

fn generate_distributions_data(
    csv_data: Vec<&PositionRecord>,
    losses_pool_size: u32,
    fees_pool_size: u32,
    former_threshold: DateTime<Utc>,
    latter_threshold: DateTime<Utc>,
    threshold: Decimal256,
) -> Result<Vec<DistributionsRecord>> {
    let mut wallet_loss_data: HashMap<Address, WalletLossRecord> = HashMap::new();
    let mut total_losses = Usd::zero();
    let mut total_fees = Usd::zero();
    for PositionRecord {
        owner,
        pnl_usd,
        total_fees_usd,
        ..
    } in csv_data
        .into_iter()
        .filter(|PositionRecord { closed_at, .. }| {
            if let Some(closed_at) = closed_at {
                former_threshold <= *closed_at && *closed_at < latter_threshold
            } else {
                false
            }
        })
    {
        let owner = owner.to_owned();
        let losses = if pnl_usd.is_negative() {
            pnl_usd.abs_unsigned()
        } else {
            Usd::zero()
        };
        let fees: Usd = total_fees_usd.abs_unsigned();

        wallet_loss_data
            .entry(owner)
            .and_modify(|value| {
                value.losses = value
                    .losses
                    .add(losses)
                    .expect("Wallet losses calculation failed.");
                value.fees = value
                    .fees
                    .add(fees)
                    .expect("Wallet fees calculation failed.");
            })
            .or_insert_with(|| WalletLossRecord {
                owner,
                losses,
                fees,
            });

        total_losses = total_losses.add(losses)?;
        total_fees = total_fees.add(fees)?;
    }

    let losses_ratio = Usd::from(u64::from(losses_pool_size)).div(total_losses)?;
    let fees_ratio = Usd::from(u64::from(fees_pool_size)).div(total_fees)?;

    Ok(wallet_loss_data
        .values()
        .filter_map(|value| {
            let losses = value
                .losses
                .mul(losses_ratio)
                .expect("Losses value calculation failed.")
                .into_decimal256();
            let fees = value
                .fees
                .mul(fees_ratio)
                .expect("Fees value calculation failed.")
                .into_decimal256();

            let losses = get_thresholded(losses, threshold, Decimal256::zero());
            let fees = get_thresholded(fees, threshold, Decimal256::zero());

            if !losses.is_zero() || !fees.is_zero() {
                Some(DistributionsRecord {
                    owner: value.owner,
                    losses,
                    fees,
                })
            } else {
                None
            }
        })
        .collect())
}

fn get_thresholded<T>(value: T, threshold: T, default: T) -> T
where
    T: PartialOrd,
{
    if value.gt(&threshold) {
        value
    } else {
        default
    }
}

pub(crate) struct WalletLossRecord {
    owner: Address,
    losses: Usd,
    fees: Usd,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DistributionsRecord {
    owner: Address,
    losses: Decimal256,
    fees: Decimal256,
}
