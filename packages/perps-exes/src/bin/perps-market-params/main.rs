use std::str::FromStr;

use anyhow::{Context, Result};
use clap::Parser;
use coingecko::Coin;
use cosmos::{Address, CosmosNetwork};
use web::axum_main;

use crate::{cli::Opt, market_param::dnf_sensitivity, slack::HttpApp};

mod cli;
mod coingecko;
mod market_param;
mod routes;
mod slack;
mod web;

fn main() -> Result<()> {
    let opt = Opt::parse();
    opt.init_logger()?;
    main_inner(opt)
}

#[tokio::main(flavor = "multi_thread")]
async fn main_inner(opt: Opt) -> Result<()> {
    match opt.sub.clone() {
        cli::SubCommand::Coins {} => {
            for coin in &Coin::all() {
                tracing::info!("{coin:?} (cmc id: {})", coin.cmc_id());
            }
        }
        cli::SubCommand::Exchanges { market_id } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let result = http_app.get_market_pair(market_id).await?;
            let result = result.data.market_pairs;
            let exchanges = http_app.get_exchanges().await?;
            let mut unsupported_exchanges = vec![];
            for market in result {
                if market.exchange_id.exchange_type().is_err() {
                    let exchange = exchanges
                        .clone()
                        .into_iter()
                        .find(|item| item.id == market.exchange_id)
                        .context(format!(
                            "Not able to find exchange id {:?}",
                            market.exchange_id
                        ))?;
                    unsupported_exchanges.push(exchange);
                }
            }
            unsupported_exchanges.sort();
            unsupported_exchanges.dedup();
            for exchange in unsupported_exchanges {
                tracing::info!(
                    "Unsupported exchange: {} (slug: {}, id: {})",
                    exchange.name,
                    exchange.slug,
                    exchange.id.0
                );
            }
        }
        cli::SubCommand::Markets {} => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let markets = vec![
                (
                    CosmosNetwork::OsmosisMainnet,
                    Address::from_str(
                        "osmo1ssw6x553kzqher0earlkwlxasfm2stnl3ms3ma2zz4tnajxyyaaqlucd45",
                    )?,
                ),
                (
                    CosmosNetwork::InjectiveMainnet,
                    Address::from_str("inj1vdu3s39dl8t5l88tyqwuhzklsx9587adv8cnn9")?,
                ),
                (
                    CosmosNetwork::SeiMainnet,
                    Address::from_str(
                        "sei18rdj3asllguwr6lnyu2sw8p8nut0shuj3sme27ndvvw4gakjnjqqper95h",
                    )?,
                ),
            ];
            let result = http_app.fetch_market_status(&markets[..]).await?;
            for market in result.markets {
                tracing::info!("{}", market.status.market_id);
            }
        }
        cli::SubCommand::Dnf { market_id } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let dnf = dnf_sensitivity(&http_app, &market_id).await?;
            tracing::info!("Computed DNF sensitivity: {dnf}");
        }
        cli::SubCommand::Serve { opt: serve_opt } => axum_main(serve_opt, opt).await?,
        cli::SubCommand::Market { out, market_id } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let exchanges = http_app.get_market_pair(market_id.clone()).await?;
            tracing::info!(
                "Total exchanges found: {} for {market_id:?}",
                exchanges.data.market_pairs.len()
            );
            let mut writer = csv::Writer::from_path(out)?;
            for exchange in exchanges.data.market_pairs {
                writer.serialize(exchange)?;
            }
            writer.flush()?;
        }
    }
    Ok(())
}
