use anyhow::Result;
use clap::Parser;
use coingecko::Coin;
use market_param::compute_dnf_sensitivity;
use web::axum_main;

use crate::{
    cli::Opt,
    coingecko::fetch_specific_spot_page_scrape,
    coingecko::{get_exchanges, map_coin_to_coingecko_id, CoingeckoApp},
};

mod cli;
mod coingecko;
mod market_param;
mod routes;
mod slack;
mod web;

fn main() -> Result<()> {
    let opt = Opt::parse();
    opt.init_logger()?;
    match opt.sub {
        cli::SubCommand::Coins {} => {
            for coin in &Coin::all() {
                tracing::info!("{coin} (coingecko id: {})", map_coin_to_coingecko_id(coin));
            }
        }
        cli::SubCommand::Scrape { coin } => {
            let app = CoingeckoApp::new()?;
            let exchanges = get_exchanges(&app, coin)?;
            tracing::info!("Successfully scraped: {} exchanges", exchanges.len());
        }
        cli::SubCommand::ScrapeLocal { path } => {
            let buffer = fs_err::read_to_string(path)?;
            let result = fetch_specific_spot_page_scrape(&buffer)?;
            tracing::info!("Successfully scraped {} exchanges locally", result.len());
        }
        cli::SubCommand::Dnf { coin } => {
            let app = CoingeckoApp::new()?;
            let exchanges = get_exchanges(&app, coin)?;
            let dnf = compute_dnf_sensitivity(exchanges)?;
            tracing::info!("Computed DNF sensitivity: {dnf}");
        }
        cli::SubCommand::Serve { opt } => axum_main(opt)?,
    }
    Ok(())
}
