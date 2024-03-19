use anyhow::{Context, Result};
use clap::Parser;
use coingecko::{get_scrape_plan_scrapy, Coin};
use std::{fs::File, io::Read};

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
            let coin = TryInto::<Coin>::try_into(coin)?;
            let coin_uri = coin.coingecko_uri();
            let app = CoingeckoApp::new()?;
            let coin_page = app.download_coin_page(&coin_uri)?;

            let plan = get_scrape_plan_scrapy(&coin_page)?;
            tracing::info!("Computed plan: {plan:?}");

            let exchanges = app.download_exchange_pages(&plan)?;
            let mut result = vec![];
            for exchange in exchanges {
                tracing::info!("Going fetch from exchange");
                let mut coin_exchanges = fetch_specific_spot_page_scrape(&exchange)?;
                result.append(&mut coin_exchanges);
            }
            tracing::info!("Successfully scraped: {} exchanges", result.len());
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
        cli::SubCommand::Test {} => {
            // let fs = File::open("/home/sibi/fpco/github/levana/levana-perps/packages/perps-exes/src/bin/perps-market-params/spot_test_page.html").unwrap();
            // let result = fetch_specific_spot_page_scrape(fs, false)?;
            // tracing::info!("Successfully scraped {} exchanges locally", result.len());

            // let app = CoingeckoApp::new()?;
            // app.download_initial_page("https://www.coingecko.com/en/coins/levana")?;
            // tracing::info!("Downloaded!");

            // let result = get_scrape_plan_scrapy()?;
            // tracing::info!("over: {result:?}");
            tracing::info!("hello world");
        }
    }
    Ok(())
}
