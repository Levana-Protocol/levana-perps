use anyhow::Result;
use clap::Parser;
use coingecko::Coin;
use market_param::compute_dnf_sensitivity;
use std::io::Read;

use crate::{
    cli::Opt,
    coingecko::fetch_specific_spot_page_scrape,
    coingecko::{get_exchanges, CoingeckoApp},
};

mod cli;
mod coingecko;
mod market_param;

fn main() -> Result<()> {
    let opt = Opt::parse();
    opt.init_logger()?;

    match opt.sub {
        cli::SubCommand::Coins {} => {
            for coin in &[Coin::Levana, Coin::Atom] {
                tracing::info!("{coin:?} (id: {})", Into::<String>::into(coin.clone()));
            }
        }
        cli::SubCommand::Scrape {
            coin,
        } => {
            let app = CoingeckoApp::new()?;
            let exchanges = get_exchanges(app, coin)?;
            tracing::info!("Successfully scraped: {} exchanges", exchanges.len());
        }
        cli::SubCommand::ScrapeLocal { path } => {
            let mut file = std::env::current_dir()?;
            file.push(path);
            let mut fs = std::fs::File::open(file)?;
            let mut buffer = String::new();
            fs.read_to_string(&mut buffer)?;

            let result = fetch_specific_spot_page_scrape(&buffer)?;
            tracing::info!("Successfully scraped {} exchanges locally", result.len());
        }
        cli::SubCommand::Dnf { coin } => {
            let app = CoingeckoApp::new()?;
            let exchanges = get_exchanges(app, coin)?;
            let dnf = compute_dnf_sensitivity(exchanges)?;
            tracing::info!("DNF sensitivity: {dnf}");
        }
    }
    Ok(())
}
