use anyhow::{anyhow, Context, Result};
use clap::Parser;
use market_param::AssetName;
use perps_exes::config::MainnetFactories;
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
    let factories = MainnetFactories::load()?.factories;
    let markets = factories
        .into_iter()
        .filter(|item| item.canonical)
        .map(|item| {
            let network = item.network;
            let network = match network {
                perps_exes::PerpsNetwork::Regular(network) => Ok(network),
                perps_exes::PerpsNetwork::DymensionTestnet => {
                    Err(anyhow!("Unsupported Dymension testnet"))
                }
            };
            network.map(|network| (network, item.address))
        })
        .filter(|item| match item {
            Ok((network, _)) => network.is_mainnet(),
            Err(_) => true,
        })
        .collect::<Result<Vec<_>>>();

    let markets = markets?;

    match opt.sub.clone() {
        cli::SubCommand::Exchanges { market_id } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let result = http_app
                .get_market_pair(AssetName(market_id.get_base()))
                .await?;
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
            let exchanges = http_app
                .get_market_pair(AssetName(market_id.get_base()))
                .await?;
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
        cli::SubCommand::ListIds { symbol } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            for symbol in http_app.get_symbol_map(&symbol).await? {
                println!("{symbol:#?}");
            }
        }
    };
    Ok(())
}
