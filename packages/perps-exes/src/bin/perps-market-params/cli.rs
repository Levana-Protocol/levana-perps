use std::{error::Error, fmt::Display, path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use clap::Subcommand;
use cosmos::{Address, CosmosNetwork};
use tracing_subscriber::{
    fmt, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

use crate::coingecko::{Coin, QuoteAsset};

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
        coin: MarketId,
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
        coin: MarketId,
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
    #[arg(long, env = "LEVANA_MPARAM_MARKET_IDS", value_delimiter = ',')]
    pub(crate) market_ids: Vec<MarketId>,
    /// Slack webhook to send alert notification
    #[arg(long, env = "LEVANA_MPARAM_SLACK_WEBHOOK")]
    pub(crate) slack_webhook: reqwest::Url,
    /// DNF threshold beyond which to raise alert
    #[arg(long, env = "LEVANA_MPARAM_DNF_THRESHOLD", default_value = "10.0")]
    pub(crate) dnf_threshold: f64,
    /// Mainnet factories
    #[clap(long, env = "LEVANA_MPARAM_MAINNET_FACTORIES", value_parser=parse_key_val::<CosmosNetwork, Address>, default_value = "osmosis-mainnet=osmo1ssw6x553kzqher0earlkwlxasfm2stnl3ms3ma2zz4tnajxyyaaqlucd45,sei-mainnet=sei18rdj3asllguwr6lnyu2sw8p8nut0shuj3sme27ndvvw4gakjnjqqper95h,injective-mainnet=inj1vdu3s39dl8t5l88tyqwuhzklsx9587adv8cnn9", use_value_delimiter=true, value_delimiter=',')]
    pub(crate) mainnet_factories: Vec<(CosmosNetwork, Address)>,
}

/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((key.parse()?, value.parse()?))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Serialize)]
pub(crate) struct MarketId {
    pub(crate) base: Coin,
    quote: QuoteAsset,
}

impl MarketId {
    pub(crate) fn base_quote(&self) -> String {
        format!("{}_{}", self.base, self.quote)
    }
}

impl Display for MarketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}", self.base, self.quote)
    }
}

impl FromStr for MarketId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut markets = s.split('_');
        let base = markets.next().context("No base asset found")?;
        let base = FromStr::from_str(base)?;
        let quote = markets.next().context("No quote asset found")?;
        let quote = FromStr::from_str(quote)?;
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
