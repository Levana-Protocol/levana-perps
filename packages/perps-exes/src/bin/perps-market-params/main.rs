use anyhow::Result;
use clap::Parser;
use coingecko::Coin;
use market_param::compute_dnf_sensitivity;
use web::axum_main;

use crate::{cli::Opt, slack::HttpApp};

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
        cli::SubCommand::Dnf { market_id } => {
            let http_app = HttpApp::new(None, opt.cmc_key.clone());
            let exchanges = http_app.get_market_pair(market_id.clone()).await?;
            tracing::info!(
                "Total exchanges found: {} for {market_id:?}",
                exchanges.data.market_pairs.len()
            );
            let dnf = compute_dnf_sensitivity(exchanges.data.market_pairs)?;
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
