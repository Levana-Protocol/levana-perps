use clap::Parser;
use cli::{Cmd, Subcommand};

mod app;
mod chain_tests;
mod cli;
mod factory;
mod faucet;
mod init_chain;
mod instantiate;
mod local_deploy;
mod localtest;
mod migrate;
mod setup_market;
mod store_code;
mod tracker;
mod util;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    main_inner().await
}

async fn main_inner() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let Cmd { opt, subcommand } = Cmd::parse();
    opt.init_logger();

    match subcommand {
        Subcommand::LocalDeploy { inner } => {
            local_deploy::go(opt, inner).await?;
        }
        Subcommand::StoreCode { inner } => store_code::go(opt, inner).await?,
        Subcommand::Instantiate { inner } => instantiate::go(opt, inner).await?,
        Subcommand::Migrate { inner } => migrate::go(opt, inner).await?,
        Subcommand::OnChainTests { inner } => localtest::go(opt, inner).await?,
        Subcommand::InitChain { inner } => init_chain::go(opt, inner).await?,
        Subcommand::SetupMarket { inner } => setup_market::go(opt, inner).await?,
    }

    Ok(())
}
