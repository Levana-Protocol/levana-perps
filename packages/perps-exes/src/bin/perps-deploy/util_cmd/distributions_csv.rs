use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::{Add, Div, Mul};
use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{load_data_from_csv, open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use cosmos::Address;
use itertools::Itertools;
use reqwest::Url;
use shared::storage::{LpToken, UnsignedDecimal, Usd};

#[derive(clap::Parser)]
pub(super) struct DistributionsCsvOpt {
    /// Directory path to contain csv files
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_BUFF_DIR")]
    pub(crate) buff_dir: PathBuf,
    /// File name of the result csv file
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_FILENAME")]
    pub(crate) filename: String,
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
    /// Provide gRPC endpoint override for factory
    #[clap(long, env = "LEVANA_DISTRIBUTIONS_FACTORY_PRIMARY_GRPC")]
    factory_primary_grpc: Url,
    /// Provide optional gRPC fallbacks URLs for factory
    #[clap(
        long,
        env = "LEVANA_DISTRIBUTIONS_FACTORY_FALLBACKS_GRPC",
        value_delimiter = ','
    )]
    factory_fallbacks_grpc: Vec<Url>,
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
        factory,
        workers,
        retries,
        losses_pool_size,
        fees_pool_size,
        factory_primary_grpc,
        factory_fallbacks_grpc,
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
            factory_primary_grpc: Some(factory_primary_grpc.clone()),
            factory_fallbacks_grpc: factory_fallbacks_grpc.clone(),
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
) -> Result<Vec<DistributionsRecord>> {
    let former_threshold = Utc::now() - Duration::weeks(1);
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
                closed_at.cmp(&former_threshold) == Ordering::Greater
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
                value.losses = value.losses.add(losses).unwrap();
                value.fees = value.fees.add(fees).unwrap();
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
            let losses =
                LpToken::from_decimal256(value.losses.mul(losses_ratio).unwrap().into_decimal256());
            let fees =
                LpToken::from_decimal256(value.fees.mul(fees_ratio).unwrap().into_decimal256());

            if losses.gt(&LpToken::from(10u64)) || fees.gt(&LpToken::from(10u64)) {
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

pub(crate) struct WalletLossRecord {
    owner: Address,
    losses: Usd,
    fees: Usd,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DistributionsRecord {
    owner: Address,
    losses: LpToken,
    fees: LpToken,
}
