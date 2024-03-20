use std::path::PathBuf;

use anyhow::Result;
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
        #[arg(value_parser = coin_parser, long)]
        coin: Coin,
        /// Skip processing elements in the page
        #[arg(long, short)]
        skip_processing: bool
    },
    /// Scrape local file
    ScrapeLocal {
        /// Local file
        #[clap(long, default_value="./spot_test_page.html")]
        path: PathBuf
    },
    /// List Supported coins with it's id
    Coins {},
    /// Compute DNF sensitivity
    Dnf {
        /// Coin string. Eg: levana
        #[arg(value_parser = coin_parser, long)]
        coin: Coin,
    }
}

fn coin_parser(arg: &str) -> Result<Coin> {
    let arg = arg.to_owned();
    arg.try_into()
}

impl Opt {
    pub(crate) fn init_logger(&self) -> Result<()> {
        let env_filter = EnvFilter::from_default_env();
        let env_filter = if std::env::var("RUST_LOG").is_ok() {
            env_filter
        } else {
            if self.verbose {
                env_filter
                    .add_directive(format!("{}=debug,info", env!("CARGO_CRATE_NAME")).parse()?)
            } else {
                env_filter.add_directive(format!("{}=info", env!("CARGO_CRATE_NAME")).parse()?)
            }
        };

        tracing_subscriber::registry()
            .with(fmt::Layer::default().and_then(env_filter))
            .init();

        tracing::debug!("Debug message!");
        Ok(())
    }
}
