use std::{path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use bigdecimal::{BigDecimal, Zero};
use chrono::{DateTime, Utc};
use cosmos::{Address, Cosmos};
use log::info;

#[derive(clap::Parser)]
pub(crate) struct Opt {
    /// Wallet to analyze
    #[clap(long)]
    wallet_addr: String,
    #[clap(long, default_value = "wallet-report.csv")]
    csv: PathBuf,
    /// Total number of datapoint you need
    #[clap(long, default_value_t = 50)]
    total_datapoints: i32,
    /// How many heights back we need to find data
    #[clap(long, default_value_t = 50)]
    lookback_height_count: i64,
}

#[derive(serde::Serialize)]
struct Record {
    height: i64,
    timestamp: DateTime<Utc>,
    balance: String,
    percentage_change: BigDecimal,
}

impl Opt {
    pub(crate) async fn run(&self, cosmos: Cosmos) -> Result<()> {
        let address = Address::from_str(&self.wallet_addr)?;
        let latest_block = cosmos.clone().get_latest_block_info().await?;
        let mut next_height = latest_block.height;
        let mut csv = csv::Writer::from_path(&self.csv)?;
        let mut old_balance = BigDecimal::zero();
        for _ in 0..self.total_datapoints {
            info!("Collecting datapoint at height {next_height}");
            let cosmos = cosmos.clone();
            let cosmos = cosmos.at_height(Some(next_height.try_into()?));
            let timestamp = cosmos.get_block_info(next_height).await?.timestamp;
            let balance = cosmos.all_balances(address).await?;

            let inj_coin = balance
                .into_iter()
                .filter(|item| item.denom == "inj")
                .next()
                .context("No balance found for injective")?;

            let new_balance = BigDecimal::from_str(&inj_coin.amount)?;
            let change = &old_balance - &new_balance;
            let percentage_diff = if &old_balance == &BigDecimal::zero() {
                BigDecimal::zero()
            } else {
                (change / old_balance) * 100
            };
            csv.serialize(Record {
                height: next_height,
                timestamp,
                balance: inj_coin.amount.to_string(),
                percentage_change: percentage_diff,
            })?;
            csv.flush()?;
            old_balance = new_balance;
            next_height = next_height - self.lookback_height_count;
        }
        Ok(())
    }
}
