use std::cmp::Ordering;
use std::path::PathBuf;

use crate::cli::Opt;
use crate::util_cmd::{load_data_from_csv, open_position_csv, OpenPositionCsvOpt, PositionRecord};
use anyhow::{anyhow, Context, Result};
use chrono::{Duration, Utc};
use itertools::Itertools;
use perps_exes::config::MainnetFactories;
use reqwest::Client;

#[derive(clap::Parser)]
pub(super) struct TopTradersOpt {
    /// Factory name
    #[clap(
        long,
        env = "LEVANA_TRADERS_FACTORIES",
        default_value = "osmomainnet1,seimainnet1,injmainnet1,ntrnmainnet1",
        use_value_delimiter = true,
        value_delimiter = ','
    )]
    factories: Vec<String>,
    /// Directory path to contain csv files
    #[clap(long, env = "LEVANA_TRADERS_BUFF_DIR")]
    pub(crate) buff_dir: PathBuf,
    /// Slack webhook to publish the notification
    #[clap(long, env = "LEVANA_TRADERS_SLACK_WEBHOOK")]
    pub(crate) slack_webhook: reqwest::Url,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: u32,
}

impl TopTradersOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(self, opt).await
    }
}

async fn go(
    TopTradersOpt {
        factories,
        buff_dir,
        slack_webhook,
        workers,
    }: TopTradersOpt,
    opt: Opt,
) -> Result<()> {
    let mainnet_factories = MainnetFactories::load()?;
    let mut notification_message = "".to_owned();
    for factory in factories {
        let active_traders_count =
            active_traders_on_factory(factory.clone(), buff_dir.clone(), opt.clone(), workers)
                .await?;
        let network_label = mainnet_factories.get(&factory)?.network;
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
) -> Result<usize> {
    let csv_filename: PathBuf = buff_dir.join(format!("{}.csv", factory.clone()));
    tracing::info!("CSV filename: {}", csv_filename.to_str().unwrap());
    if let Err(e) = open_position_csv(
        opt,
        OpenPositionCsvOpt {
            factory,
            csv: csv_filename.clone(),
            workers,
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
    tracing::info!("Here's the csv data length: {:?}", active_trader_count);
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
