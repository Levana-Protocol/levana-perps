use std::str::FromStr;

use anyhow::{Context, Result};
use clap::Parser;
use coingecko::Coin;
use cosmos::{Address, CosmosNetwork};
use web::axum_main;

use crate::{
    cli::Opt,
    market_param::{compute_dnf_notify, dnf_sensitivity},
    slack::HttpApp,
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
            let result = http_app.get_market_pair(market_id.clone()).await?;
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
            for exchange in &unsupported_exchanges {
                tracing::info!(
                    "Unsupported exchange: {} (slug: {}, id: {})",
                    exchange.name,
                    exchange.slug,
                    exchange.id.0
                );
            }

            if unsupported_exchanges.is_empty() {
                tracing::info!("All exchanges are supported for {market_id}");
            } else {
                tracing::info!(
                    "Total unsupported exchanges for {market_id}: {}",
                    unsupported_exchanges.len()
                );
            }
        }
        cli::SubCommand::Markets { market_ids } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let markets = [
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
            tracing::info!("Skipping {0} deployed markets", market_ids.len());
            let result = http_app.fetch_market_status(&markets[..]).await?;
            let markets = result
                .markets
                .into_iter()
                .filter(|market| !market_ids.contains(&market.status.market_id));

            for market in markets {
                tracing::info!("{}", market.status.market_id);
            }
        }
        cli::SubCommand::Dnf { market_id } => {
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
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let dnf = dnf_sensitivity(&http_app, &market_id).await?;
            let market_config = http_app.fetch_market_status(&markets).await?;
            let configured_dnf = market_config.get_chain_dnf(&market_id);
            let configured_dnf = match configured_dnf {
                Ok(configured_dnf) => {
                    tracing::info!("Configured DNF sensitivity: {}", configured_dnf);
                    configured_dnf
                }
                Err(err) => {
                    tracing::warn!("{err}");
                    0.0
                }
            };

            let dnf_notify = compute_dnf_notify(dnf, configured_dnf, 100.0, 50.0);
            tracing::info!("Computed DNF sensitivity: {}", dnf_notify.computed_dnf);

            if configured_dnf != 0.0 {
                tracing::info!(
                    "Percentage diff ({market_id}): {}",
                    dnf_notify.percentage_diff
                );
            }
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
