use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{load_data_from_csv, open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use cosmos::CosmosNetwork;
use itertools::Itertools;
use perps_exes::{config::MainnetFactories, PerpsNetwork};
use reqwest::{Client, Url};

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
    /// Number of retries when an error occurs while generating a csv file
    #[clap(long, env = "LEVANA_TRADERS_RETRIES", default_value_t = 3)]
    retries: u32,
    /// Provide gRPC endpoint override for osmosis mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_OSMOSIS_MAINNET_PRIMARY_GRPC",
        default_value = "https://osmo-priv-grpc.kingnodes.com"
    )]
    osmosis_mainnet_primary_grpc: Url,
    /// Provide optional gRPC fallbacks URLs for osmosis mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_OSMOSIS_MAINNET_FALLBACKS_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.osmosis-1.mesa-grpc.newmetric.xyz,http://146.190.0.132:9090,https://grpc.osmosis.zone,http://osmosis-grpc.polkachu.com:12590",
        value_delimiter = ','
    )]
    osmosis_mainnet_fallbacks_grpc: Vec<Url>,
    /// Provide gRPC endpoint override for injective mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_INJECTIVE_MAINNET_PRIMARY_GRPC",
        default_value = "https://inj-priv-grpc.kingnodes.com"
    )]
    injective_mainnet_primary_grpc: Url,
    /// Provide optional gRPC fallbacks URLs for injective mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_INJECTIVE_MAINNET_FALLBACKS_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.injective-1.mesa-grpc.newmetric.xyz,http://injective-grpc.polkachu.com:14390",
        value_delimiter = ','
    )]
    injective_mainnet_fallbacks_grpc: Vec<Url>,
    /// Provide gRPC endpoint override for neutron mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_NEUTRON_MAINNET_PRIMARY_GRPC",
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.neutron-1.mesa-grpc.newmetric.xyz"
    )]
    neutron_mainnet_primary_grpc: Url,
    /// Provide optional gRPC fallbacks URLs for neutron mainnet
    #[clap(
        long,
        env = "LEVANA_TRADERS_NEUTRON_MAINNET_FALLBACKS_GRPC",
        default_value = "http://neutron-grpc.rpc.p2p.world:3001,http://grpc-kralum.neutron-1.neutron.org",
        value_delimiter = ','
    )]
    neutron_mainnet_fallbacks_grpc: Vec<Url>,
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
        retries,
        osmosis_mainnet_primary_grpc,
        osmosis_mainnet_fallbacks_grpc,
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
            retries,
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
    retries: u32,
    factory_primary_grpc: Url,
    factory_fallbacks_grpc: Vec<Url>,
) -> Result<usize> {
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

    let start_date = (Utc::now() - chrono::Duration::days(1)).date_naive();
    let start_date = start_date
        .and_hms_opt(0, 0, 0)
        .expect("Error adding hours/minutes/seconds")
        .and_utc();
    let end_date = start_date + chrono::Duration::days(1);

    let active_trader_count = csv_data
        .values()
        .filter_map(
            |PositionRecord {
                 opened_at,
                 closed_at,
                 owner,
                 ..
             }| match (opened_at, closed_at) {
                (opened_at, _) if opened_at >= &start_date && opened_at < &end_date => Some(owner),
                (_, Some(closed_at)) if closed_at >= &start_date && closed_at < &end_date => {
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
