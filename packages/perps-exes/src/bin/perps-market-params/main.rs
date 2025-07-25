use anyhow::{anyhow, Context, Result};
use clap::Parser;
use market_param::AssetName;
use perps_exes::config::MainnetFactories;
use web::axum_main;

use crate::{
    cli::Opt,
    coingecko::ExchangeKind,
    market_param::{
        compute_dnf_notify, dnf_sensitivity, dnf_sensitivity_to_max_leverage, DnfInNotional,
        NotionalAsset,
    },
    slack::HttpApp,
};

mod cli;
mod coingecko;
mod market_param;
mod routes;
mod s3;
mod slack;
mod web;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
                perps_exes::PerpsNetwork::NibiruTestnet => {
                    Err(anyhow!("Unsupported Nibiru testnet"))
                }
                perps_exes::PerpsNetwork::RujiraDevnet => Err(anyhow!("Unsupported Rujira devnet")),
                perps_exes::PerpsNetwork::RujiraTestnet => {
                    Err(anyhow!("Unsupported Rujira testnet"))
                }
                perps_exes::PerpsNetwork::RujiraMainnet => {
                    Err(anyhow!("Unsupported Rujira mainnet"))
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
        cli::SubCommand::CurrentMarketDnf { market_id } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let dnf = dnf_sensitivity(&http_app, &market_id).await?;
            let max_leverage = dnf_sensitivity_to_max_leverage(dnf.dnf_in_usd);

            tracing::info!("DNF: {}", dnf.dnf_in_notional);
            tracing::info!("Min depth liquidity: {}", dnf.min_depth_liquidity);
            tracing::info!("Max leverage: {max_leverage:?}");
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
                    DnfInNotional(0.0)
                }
            };
            let actual_configured_max_leverage = market_config
                .get_chain_max_leverage(&market_id)
                .context("No max_leverage configured")?;
            let recommended_configured_max_leverage = dnf_sensitivity_to_max_leverage(
                configured_dnf
                    .as_asset_amount(NotionalAsset(market_id.get_notional()), &http_app)
                    .await?,
            );

            tracing::info!("Notional asset: {}", market_id.get_notional());
            tracing::info!("DNF in USD: {}", dnf.dnf_in_usd);

            let max_leverage = dnf_sensitivity_to_max_leverage(dnf.dnf_in_usd);

            let dnf_notify = compute_dnf_notify(dnf.dnf_in_notional, configured_dnf, 100.0, 50.0);
            tracing::info!("Computed DNF sensitivity: {}", dnf_notify.computed_dnf);

            if configured_dnf.0 != 0.0 {
                tracing::info!(
                    "Percentage diff ({market_id}): {}",
                    dnf_notify.percentage_diff
                );
            }

            tracing::info!("Configured max_leverage: {actual_configured_max_leverage:?}");
            tracing::info!(
                "Recommended max_leverage (based on configured DNF): {recommended_configured_max_leverage:?}"
            );
            tracing::info!("Recommended max_leverage (based on computed current day market DNF): {max_leverage:?}");
        }
        cli::SubCommand::Serve { opt: serve_opt } => axum_main(serve_opt, opt).await?,
        cli::SubCommand::Market {
            out,
            market_id,
            cex_only,
        } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let exchanges = http_app
                .get_market_pair(AssetName(market_id.get_base()))
                .await?;
            tracing::info!("All exchanges for {market_id:?}: {}", exchanges.len());

            let exchanges: Vec<_> = exchanges
                .into_iter()
                .filter_map(|item| {
                    let exchange_type = item
                        .exchange_id
                        .exchange_type()
                        .is_ok_and(|exchange| exchange == ExchangeKind::Cex);
                    if cex_only == exchange_type {
                        Some(item)
                    } else {
                        None
                    }
                })
                .collect();
            tracing::info!(
                "Total exchanges found: {} for {market_id:?} (Only Cex {cex_only})",
                exchanges.len()
            );
            let mut writer = csv::Writer::from_path(out)?;
            for exchange in exchanges {
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
