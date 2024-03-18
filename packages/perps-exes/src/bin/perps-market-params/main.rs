use anyhow::{Context, Result};
use clap::Parser;
use coingecko::{Coin, get_scrape_plan_scrapy};
use std::fs::File;

use crate::{cli::Opt, coingecko::fetch_specific_spot_page_scrape, coingecko::CoingeckoApp};

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
        cli::SubCommand::Scrape {
            coin,
            skip_processing,
        } => {
            // let coin = TryInto::<Coin>::try_into(coin)?;
            // let app = CoingeckoApp::new()?;
            // let plan = app.get_scrape_plan(coin.coingecko_uri().as_str())?;
            // tracing::debug!("Scrape plan: {plan:?}");
            // let result = app.apply_scrape_plan(plan, skip_processing)?;
            // tracing::info!("Successfully scraped");
            // for exchange in result {
            //     tracing::info!("{}", exchange.name);
            // }
            todo!()
        }
        cli::SubCommand::ScrapeLocal { path } => {
            let mut file = std::env::current_dir()?;
            file.push(path);
            let fs = std::fs::File::open(file)?;

            let result = fetch_specific_spot_page_scrape(fs, false)?;
            tracing::info!("Successfully scraped {} exchanges locally", result.len());
        }
        cli::SubCommand::Test {} => {
            // let fs = File::open("/home/sibi/fpco/github/levana/levana-perps/packages/perps-exes/src/bin/perps-market-params/spot_test_page.html").unwrap();
            // let result = fetch_specific_spot_page_scrape(fs, false)?;
            // tracing::info!("Successfully scraped {} exchanges locally", result.len());

            // let app = CoingeckoApp::new()?;
            // app.download_initial_page("https://www.coingecko.com/en/coins/levana")?;
            // tracing::info!("Downloaded!");

            let result = get_scrape_plan_scrapy()?;
            tracing::info!("over: {result:?}");

        }
    }
    Ok(())
}
