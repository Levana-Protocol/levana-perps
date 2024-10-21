use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::{bail, Result};
use chrono::Utc;
use cosmos::Address;
use cosmos::CosmosNetwork;
use perps_exes::{config::MainnetFactories, PerpsNetwork};
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
    /// Provide gRPC endpoint override for osmosis mainnet
    #[clap(
        long,
        env = "LEVANA_FEES_OSMOSIS_MAINNET_PRIMARY_GRPC",
        default_value = "https://osmo-priv-grpc.kingnodes.com"
    )]
    osmosis_mainnet_primary_grpc: Url,
    /// Provide optional gRPC fallbacks URLs for osmosis mainnet
    #[clap(
        long,
        env = "LEVANA_FEES_OSMOSIS_MAINNET_FALLBACKS_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.osmosis-1.mesa-grpc.newmetric.xyz,http://146.190.0.132:9090,https://grpc.osmosis.zone,http://osmosis-grpc.polkachu.com:12590",
        value_delimiter = ','
    )]
    osmosis_mainnet_fallbacks_grpc: Vec<Url>,
    /// Provide gRPC endpoint override for injective mainnet
    #[clap(
        long,
        env = "LEVANA_FEES_INJECTIVE_MAINNET_PRIMARY_GRPC",
        default_value = "https://inj-priv-grpc.kingnodes.com"
    )]
    injective_mainnet_primary_grpc: Url,
    /// Provide optional gRPC fallbacks URLs for injective mainnet
    #[clap(
        long,
        env = "LEVANA_FEES_INJECTIVE_MAINNET_FALLBACKS_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.injective-1.mesa-grpc.newmetric.xyz,http://injective-grpc.polkachu.com:14390",
        value_delimiter = ','
    )]
    injective_mainnet_fallbacks_grpc: Vec<Url>,
    /// Provide gRPC endpoint override for neutron mainnet
    #[clap(
        long,
        env = "LEVANA_FEES_NEUTRON_MAINNET_PRIMARY_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.neutron-1.mesa-grpc.newmetric.xyz"
    )]
    neutron_mainnet_primary_grpc: Url,
    /// Provide optional gRPC fallbacks URLs for neutron mainnet
    #[clap(
        long,
        env = "LEVANA_FEES_NEUTRON_MAINNET_FALLBACKS_GRPC",
        default_value = "http://neutron-grpc.rpc.p2p.world:3001,http://grpc-kralum.neutron-1.neutron.org",
        value_delimiter = ','
    )]
    neutron_mainnet_fallbacks_grpc: Vec<Url>,
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
        osmosis_mainnet_primary_grpc,
        osmosis_mainnet_fallbacks_grpc,
        injective_mainnet_primary_grpc,
        injective_mainnet_fallbacks_grpc,
        neutron_mainnet_primary_grpc,
        neutron_mainnet_fallbacks_grpc,
    }: TradingFeesOpt,
    opt: Opt,
) -> Result<()> {
    let mainnet_factories = MainnetFactories::load()?;
    let (factory_name, factory) = (factory.clone(), mainnet_factories.get(&factory)?);

    let csv_filename: PathBuf = buff_dir.join(format!("{}.csv", factory_name.clone()));
    tracing::info!("CSV filename: {}", csv_filename.as_path().display());

    if let Some(parent) = csv_filename.parent() {
        fs_err::create_dir_all(parent)?;
    }

    let (factory_primary_grpc, factory_fallbacks_grpc) = match factory.network {
        PerpsNetwork::Regular(CosmosNetwork::OsmosisMainnet) => (
            osmosis_mainnet_primary_grpc.clone(),
            osmosis_mainnet_fallbacks_grpc.clone(),
        ),
        PerpsNetwork::Regular(CosmosNetwork::InjectiveMainnet) => (
            injective_mainnet_primary_grpc.clone(),
            injective_mainnet_fallbacks_grpc.clone(),
        ),
        PerpsNetwork::Regular(CosmosNetwork::NeutronMainnet) => (
            neutron_mainnet_primary_grpc.clone(),
            neutron_mainnet_fallbacks_grpc.clone(),
        ),
        _ => bail!("Unsupported network: {}", factory.network),
    };
    let mut attempted_retries = 0;
    while let Err(e) = open_position_csv(
        opt.clone(),
        OpenPositionCsvOpt {
            factory: factory_name.clone(),
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

    let start_date = (Utc::now() - chrono::Duration::days(1)).date_naive();
    let start_date = start_date
        .and_hms_opt(0, 0, 0)
        .expect("Error adding hours/minutes/seconds")
        .and_utc();
    let end_date = start_date + chrono::Duration::days(1);

    let mut fees: HashMap<Address, Usd> = HashMap::new();
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
        *entry = entry.checked_add(trading_fee_usd)?;
    }

    let value = serde_json::json!(
        {
            "timestamp": start_date,
            "wallets": fees
                .into_iter()
                .map(|(recipient, amount)| serde_json::json!({
                    "wallet": recipient.to_string(),
                    "trading_fees_in_usd": amount.to_string(),
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
