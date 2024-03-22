use std::{path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use clap::Subcommand;
use tracing_subscriber::{
    fmt, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

use crate::coingecko::Coin;

#[derive(clap::Parser)]
pub(crate) struct Opt {
    #[clap(long)]
    verbose: bool,
    #[clap(subcommand)]
    pub sub: SubCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SubCommand {
    /// Scrape particular coin
    Scrape {
        /// Coin string. Eg: levana
        #[arg(long)]
        coin: Coin,
    },
    /// Scrape local file
    ScrapeLocal {
        /// Local file
        #[clap(long, default_value = "./spot_test_page.html")]
        path: PathBuf,
    },
    /// List supported coins with their IDs
    Coins {},
    /// Compute DNF sensitivity
    Dnf {
        /// Coin string. Eg: levana
        #[arg(long)]
        coin: Coin,
    },
    /// Serve web application
    Serve {
        #[clap(flatten)]
        opt: ServeOpt,
    },
}

#[derive(clap::Parser, Clone, Debug)]
pub(crate) struct ServeOpt {
    /// Coins to track
    #[arg(long, env = "LEVANA_MPARAM_COINS", value_delimiter = ',')]
    pub(crate) coins: Vec<MarketId>,
    /// Slack webhook to send alert notification
    #[arg(long, env = "LEVANA_MPARAM_SLACK_WEBHOOK")]
    pub(crate) slack_webhook: reqwest::Url,
    /// DNF threshold beyond which to raise alert
    #[arg(long, env = "LEVANA_MPARAM_DNF_THRESHOLD", default_value = "10.0")]
    pub(crate) dnf_threshold: f64,
}

pub(crate) struct MarketId {
    base: Coin,
    quote: String,
}

impl FromStr for MarketId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut markets = s.split('_');
        let base = markets.next().context("No base asset found")?;
        let base = FromStr::from_str(base)?;
        let quote = markets.next().context("No quote asset found")?.to_owned();
        Ok(MarketId { base, quote })
    }
}

impl Opt {
    pub(crate) fn init_logger(&self) -> Result<()> {
        let env_filter = EnvFilter::from_default_env();
        let env_filter = if std::env::var("RUST_LOG").is_ok() {
            env_filter
        } else if self.verbose {
            env_filter.add_directive(format!("{}=debug,info", env!("CARGO_CRATE_NAME")).parse()?)
        } else {
            env_filter.add_directive(format!("{}=info", env!("CARGO_CRATE_NAME")).parse()?)
        };

        tracing_subscriber::registry()
            .with(fmt::Layer::default().and_then(env_filter))
            .init();

        tracing::debug!("Debug message!");
        Ok(())
    }
}
