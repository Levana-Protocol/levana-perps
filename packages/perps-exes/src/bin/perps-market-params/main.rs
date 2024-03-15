use anyhow::{Context, Result};
use clap::Parser;
use coingecko::Coin;

use crate::{cli::Opt, coingecko::CoingeckoApp};

mod cli;
mod coingecko;
mod market_param;

fn main() -> Result<()> {
    let opt = Opt::parse();
    opt.init_logger()?;

    match opt.sub {
        cli::SubCommand::Coins {} => {
            for coin in &[Coin::Levana, Coin::Atom] {
                tracing::info!("{coin:?} : {}", Into::<String>::into(coin.clone()));
            }
        }

        cli::SubCommand::Scrape { coin } => {
            let coin = TryInto::<Coin>::try_into(coin)?;
            let app = CoingeckoApp::new()?;
            let plan = app.get_scrape_plan(coin.coingecko_uri().as_str())?;
            tracing::debug!("Scrape plan: {plan:?}");
            let result = app.apply_scrape_plan(plan)?;
            tracing::info!("Successfully scraped");
            for exchange in result {
                tracing::info!("{}", exchange.name);
            }
        }
        cli::SubCommand::ScrapeLocal { path } => {
            tracing::warn!("This might take more than 5 minutes");
            let mut file = std::env::current_dir()?;
            file.push(path);
            let uri = format!("file://{}", file.to_str().context("Invalid uri")?);
            let app = CoingeckoApp::new()?;
            let result = app.fetch_specific_spot_page(Some(uri), false)?;
            tracing::info!("Successfully scraped {} exchanges locally", result.len());
        }
    }
    Ok(())
}
