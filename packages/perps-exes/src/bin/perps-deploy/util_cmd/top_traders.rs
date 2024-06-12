use std::cmp::Ordering;
use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{load_data_from_csv, open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Duration, Utc};
use cosmos::CosmosNetwork;
use itertools::Itertools;
use perps_exes::{config::MainnetFactories, PerpsNetwork};
use reqwest::Client;

#[derive(clap::Parser)]
pub(super) struct TopTradersOpt {
    /// Directory path to contain csv files
    #[clap(long, env = "LEVANA_TRADERS_BUFF_DIR")]
    pub(crate) buff_dir: PathBuf,
    /// Slack webhook to publish the notification
    #[clap(long, env = "LEVANA_TRADERS_SLACK_WEBHOOK")]
    pub(crate) slack_webhook: reqwest::Url,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: u32,
    /// Provide gRPC endpoint override for osmosis mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_OSMOSIS_MAINNET_PRIMARY_GRPC",
        default_value = "https://osmo-priv-grpc.kingnodes.com"
    )]
    osmosis_mainnet_primary_grpc: String,
    /// Provide optional gRPC fallbacks URLs for osmosis mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_OSMOSIS_MAINNET_FALLBACKS_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.osmosis-1.mesa-grpc.newmetric.xyz,http://146.190.0.132:9090,https://grpc.osmosis.zone,http://osmosis-grpc.polkachu.com:12590",
        value_delimiter = ','
    )]
    osmosis_mainnet_fallbacks_grpc: Vec<String>,
    /// Provide gRPC endpoint override for sei mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_SEI_MAINNET_PRIMARY_GRPC",
        default_value = "https://sei-priv-grpc.kingnodes.com"
    )]
    sei_mainnet_primary_grpc: String,
    /// Provide optional gRPC fallbacks URLs for sei mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_SEI_MAINNET_FALLBACKS_GRPC",
        default_value = "http://sei-grpc.polkachu.com:11990,https://grpc.sei-apis.com,https://sei-grpc.brocha.in",
        value_delimiter = ','
    )]
    sei_mainnet_fallbacks_grpc: Vec<String>,
    /// Provide gRPC endpoint override for injective mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_INJECTIVE_MAINNET_PRIMARY_GRPC",
        default_value = "https://inj-priv-grpc.kingnodes.com"
    )]
    injective_mainnet_primary_grpc: String,
    /// Provide optional gRPC fallbacks URLs for injective mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_INJECTIVE_MAINNET_FALLBACKS_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.injective-1.mesa-grpc.newmetric.xyz,http://injective-grpc.polkachu.com:14390",
        value_delimiter = ','
    )]
    injective_mainnet_fallbacks_grpc: Vec<String>,
    /// Provide gRPC endpoint override for neutron mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_NEUTRON_MAINNET_PRIMARY_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.neutron-1.mesa-grpc.newmetric.xyz"
    )]
    neutron_mainnet_primary_grpc: String,
    /// Provide optional gRPC fallbacks URLs for neutron mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_NEUTRON_MAINNET_FALLBACKS_GRPC",
        default_value = "http://neutron-grpc.rpc.p2p.world:3001,http://grpc-kralum.neutron-1.neutron.org",
        value_delimiter = ','
    )]
    neutron_mainnet_fallbacks_grpc: Vec<String>,
}

impl TopTradersOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(self, opt).await
    }
}

async fn go(
    TopTradersOpt {
        buff_dir,
        slack_webhook,
        workers,
        osmosis_mainnet_primary_grpc,
        osmosis_mainnet_fallbacks_grpc,
        sei_mainnet_primary_grpc,
        sei_mainnet_fallbacks_grpc,
        injective_mainnet_primary_grpc,
        injective_mainnet_fallbacks_grpc,
        neutron_mainnet_primary_grpc,
        neutron_mainnet_fallbacks_grpc,
    }: TopTradersOpt,
    opt: Opt,
) -> Result<()> {
    let mainnet_factories = MainnetFactories::load()?.factories;
    let mut notification_message = "".to_owned();
    for factory in mainnet_factories {
        if !factory.canonical {
            continue;
        }

        let ident = factory.ident.with_context(|| {
            format!("Factory identifier does not exist for {}", factory.network)
        })?;
        let (factory_primary_grpc, factory_fallbacks_grpc) = match factory.network {
            PerpsNetwork::Regular(CosmosNetwork::OsmosisMainnet) => (
                osmosis_mainnet_primary_grpc.clone(),
                osmosis_mainnet_fallbacks_grpc.clone(),
            ),
            PerpsNetwork::Regular(CosmosNetwork::SeiMainnet) => (
                sei_mainnet_primary_grpc.clone(),
                sei_mainnet_fallbacks_grpc.clone(),
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
        let active_traders_count = active_traders_on_factory(
            ident,
            buff_dir.clone(),
            opt.clone(),
            workers,
            factory_primary_grpc,
            factory_fallbacks_grpc,
        )
        .await?;
        let network_label = factory.network;
        notification_message += format!(
            "*{}* traders were active on _*{}*_\n",
            active_traders_count,
            get_factory_name_from_network_label(network_label.to_string())?
        )
        .as_str();
    }
    send_slack_notification(
        slack_webhook,
        "Number of active traders (Last 24 hours)".to_owned(),
        notification_message,
    )
    .await?;
    Ok(())
}

async fn active_traders_on_factory(
    factory: String,
    buff_dir: PathBuf,
    opt: Opt,
    workers: u32,
    factory_primary_grpc: String,
    factory_fallbacks_grpc: Vec<String>,
) -> Result<usize> {
    let csv_filename: PathBuf = buff_dir.join(format!("{}.csv", factory.clone()));
    tracing::info!("CSV filename: {}", csv_filename.as_path().display());

    if let Err(e) = open_position_csv(
        opt,
        OpenPositionCsvOpt {
            factory,
            csv: csv_filename.clone(),
            workers,
            factory_primary_grpc: Some(factory_primary_grpc),
            factory_fallbacks_grpc,
        },
    )
    .await
    {
        tracing::error!("Error while generating open position csv file: {}", e);
    }

    tracing::info!("Reading csv data");
    let csv_data = load_data_from_csv(&csv_filename).with_context(|| {
        format!(
            "Unable to load old CSV data from {}",
            csv_filename.display()
        )
    })?;
    let former_threshold = Utc::now() - Duration::hours(24);
    let active_trader_count = csv_data
        .values()
        .filter_map(
            |PositionRecord {
                 opened_at,
                 closed_at,
                 owner,
                 ..
             }| match (opened_at, closed_at) {
                (opened_at, _) if opened_at.cmp(&former_threshold) == Ordering::Greater => {
                    Some(owner)
                }
                (_, Some(closed_at)) if closed_at.cmp(&former_threshold) == Ordering::Greater => {
                    Some(owner)
                }
                (_, _) => None,
            },
        )
        .unique()
        .count();
    tracing::info!(
        "Here's the count of active traders: {:?}",
        active_trader_count
    );
    Ok(active_trader_count)
}

pub(crate) async fn send_slack_notification(
    webhook: reqwest::Url,
    header: String,
    message: String,
) -> anyhow::Result<()> {
    let value = serde_json::json!(
    {
        "text": "Active traders in 24 hours",
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

fn get_factory_name_from_network_label(network: String) -> anyhow::Result<String> {
    if let Some(factory) = network.split('-').next() {
        let mut factory_chars = factory.chars();
        match factory_chars.next() {
            None => Ok(String::new()),
            Some(char) => Ok(char.to_uppercase().chain(factory_chars).collect()),
        }
    } else {
        Err(anyhow!(
            "Can not get factory name from the network label: {}",
            network
        ))
    }
}
