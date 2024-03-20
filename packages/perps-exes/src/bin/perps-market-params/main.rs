use anyhow::Result;
use clap::Parser;
use coingecko::Coin;
use market_param::compute_dnf_sensitivity;
use std::io::Read;

use crate::{
    cli::Opt,
    coingecko::fetch_specific_spot_page_scrape,
    coingecko::{get_exchanges, CoingeckoApp},
    market_param::{get_current_dnf, load_markets_config},
};

mod cli;
mod coingecko;
mod market_param;

fn main() -> Result<()> {
    let opt = Opt::parse();
    opt.init_logger()?;
    let market_config = include_bytes!("../../../assets/market-config-updates.yaml");
    match opt.sub {
        cli::SubCommand::Coins {} => {
            for coin in &[Coin::Levana, Coin::Atom] {
                tracing::info!("{coin:?} (id: {})", Into::<String>::into(coin.clone()));
            }
        }
        cli::SubCommand::Scrape { coin } => {
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
            let market_config = load_markets_config(market_config)?;
            let configured_dnf = get_current_dnf(&market_config, &coin);
            if let Some(configured_dnf) = configured_dnf {
                tracing::info!("Configured DNF sensitivity: {configured_dnf}");
            }
            let exchanges = get_exchanges(app, coin)?;
            let dnf = compute_dnf_sensitivity(exchanges)?;
            tracing::info!("Computed DNF sensitivity: {dnf}");
        }
    }
    Ok(())
}
