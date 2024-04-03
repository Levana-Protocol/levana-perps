use std::{error::Error, path::PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;
use cosmos::{Address, CosmosNetwork};
use shared::storage::MarketId;
use tracing_subscriber::{
    fmt, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

#[derive(clap::Parser, Clone)]
pub(crate) struct Opt {
    /// Verbose flag
    #[clap(long)]
    verbose: bool,
    /// CMC key
    #[clap(long, env = "LEVANA_MPARAM_CMC_KEY")]
    pub(crate) cmc_key: String,
    #[clap(subcommand)]
    pub(crate) sub: SubCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum SubCommand {
    /// List supported coins with their IDs
    Coins {},
    /// Compute DNF sensitivity
    Dnf {
        /// Market ID. Eg: ATOM_USD
        #[arg(long)]
        market_id: MarketId,
    },
    /// Download market data in csv
    Market {
        /// Destination file location
        #[arg(long, default_value = "market.csv")]
        out: PathBuf,
        /// Market ID. Eg: ATOM_USD
        #[arg(long)]
        market_id: MarketId,
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
    /// DNF increase threshold beyond which to raise alert
    #[arg(
        long,
        env = "LEVANA_MPARAM_DNF_INCREASE_THRESHOLD",
        default_value = "50.0"
    )]
    pub(crate) dnf_increase_threshold: f64,
    /// DNF decrease beyond which to raise alert
    #[arg(
        long,
        env = "LEVANA_MPARAM_DNF_DECREASE_THRESHOLD",
        default_value = "10.0"
    )]
    pub(crate) dnf_decrease_threshold: f64,
    /// Mainnet factories
    #[clap(long, env = "LEVANA_MPARAM_MAINNET_FACTORIES", value_parser=parse_key_val::<CosmosNetwork, Address>, default_value = "osmosis-mainnet=osmo1ssw6x553kzqher0earlkwlxasfm2stnl3ms3ma2zz4tnajxyyaaqlucd45,sei-mainnet=sei18rdj3asllguwr6lnyu2sw8p8nut0shuj3sme27ndvvw4gakjnjqqper95h,injective-mainnet=inj1vdu3s39dl8t5l88tyqwuhzklsx9587adv8cnn9", use_value_delimiter=true, value_delimiter=',')]
    pub(crate) mainnet_factories: Vec<(CosmosNetwork, Address)>,
}

/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> anyhow::Result<(T, U)>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let (key, value) = s
        .split_once('=')
        .with_context(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((key.parse()?, value.parse()?))
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
