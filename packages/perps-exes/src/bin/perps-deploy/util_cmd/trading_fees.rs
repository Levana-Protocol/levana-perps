use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::Result;
use chrono::{DateTime, Utc};
use cosmos::Address;
use perpswap::storage::{UnsignedDecimal, Usd};
use reqwest::Url;

#[derive(clap::Parser)]
pub(super) struct TradingFeesOpt {
    /// Directory path to contain csv files
    #[clap(
        long,
        env = "LEVANA_FEES_BUFF_DIR",
        default_value = ".cache/trading-incentives"
    )]
    pub(crate) buff_dir: PathBuf,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: u32,
    /// Number of retries when an error occurs while generating a csv file
    #[clap(long, env = "LEVANA_FEES_RETRIES", default_value_t = 3)]
    retries: u32,
    /// Factory identifier
    #[clap(long, default_value = "osmomainnet1", env = "LEVANA_FEES_FACTORY")]
    factory: String,
    /// Provide optional gRPC fallbacks URLs for factory
    #[clap(long, env = "COSMOS_GRPC_FALLBACKS", value_delimiter = ',')]
    cosmos_grpc_fallbacks: Vec<Url>,
}

impl TradingFeesOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(self, opt).await
    }
}

async fn go(
    TradingFeesOpt {
        buff_dir,
        workers,
        retries,
        factory,
        cosmos_grpc_fallbacks,
    }: TradingFeesOpt,
    opt: Opt,
) -> Result<()> {
    let csv_filename: PathBuf = buff_dir.join(format!("{}.csv", factory.clone()));
    tracing::info!("CSV filename: {}", csv_filename.as_path().display());

    if let Some(parent) = csv_filename.parent() {
        fs_err::create_dir_all(parent)?;
    }

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

    let start_date = (Utc::now() - chrono::Duration::days(1)).date_naive();
    let start_date = start_date
        .and_hms_opt(0, 0, 0)
        .expect("Error adding hours/minutes/seconds")
        .and_utc();
    let end_date = start_date + chrono::Duration::days(1);

    let mut fees: HashMap<Address, WalletFees> = HashMap::new();
    let mut timestamp: DateTime<Utc> = DateTime::default();
    for record in csv::Reader::from_path(&csv_filename)?.into_deserialize() {
        let PositionRecord {
            closed_at,
            owner,
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
        let entry = fees.entry(owner).or_default();
        *entry = WalletFees {
            fees: entry.fees.checked_add(trading_fee_usd)?,
            timestamp: entry.timestamp.max(closed_at),
        };
        timestamp = timestamp.max(closed_at);
    }

    let value = serde_json::json!(
        {
            "timestamp": timestamp,
            "wallets": fees
                .into_iter()
                .map(|(recipient, WalletFees { fees, timestamp })| serde_json::json!({
                    "timestamp": timestamp,
                    "wallet": recipient.to_string(),
                    "trading_fees_in_usd": fees.to_string(),
                }))
                .collect::<Vec<_>>()
        }
    );

    let output = format!("{}-trading-fees.json", start_date.date_naive());
    let mut output = {
        let file = std::fs::File::create(output)?;
        BufWriter::new(file)
    };
    serde_json::to_writer(&mut output, &value)?;
    output.flush()?;

    Ok(())
}

#[derive(Default)]
struct WalletFees {
    fees: Usd,
    timestamp: DateTime<Utc>,
}
