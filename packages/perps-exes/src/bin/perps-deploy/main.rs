use clap::Parser;
use cli::{Cmd, Subcommand, TestnetSub};

mod app;
mod chain_tests;
mod cli;
mod factory;
mod faucet;
mod init_chain;
mod instantiate;
mod instantiate_rewards;
mod local_deploy;
mod localtest;
mod mainnet;
mod migrate;
mod migrate_rewards;
mod store_code;
mod testnet;
mod tracker;
mod util;
mod util_cmd;

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
        Subcommand::OnChainTests { inner } => localtest::go(opt, inner).await?,
        Subcommand::Testnet { inner } => match inner {
            TestnetSub::StoreCode { inner } => store_code::go(opt, inner).await?,
            TestnetSub::Instantiate { inner } => instantiate::go(opt, inner).await?,
            TestnetSub::Migrate { inner } => migrate::go(opt, inner).await?,
            TestnetSub::InitChain { inner } => init_chain::go(opt, inner).await?,
            TestnetSub::InstantiateRewards { inner } => instantiate_rewards::go(opt, inner).await?,
            TestnetSub::MigrateRewards { inner } => migrate_rewards::go(opt, inner).await?,
            TestnetSub::Deposit { inner } => inner.go(opt).await?,
            TestnetSub::EnableMarket { inner } => inner.go(opt).await?,
            TestnetSub::DisableMarketAt { inner } => inner.go(opt).await?,
            TestnetSub::CloseAllPositions { inner } => inner.go(opt).await?,
            TestnetSub::AddMarket { inner } => inner.go(opt).await?,
            TestnetSub::UpdateMarketConfigs { inner } => inner.go(opt).await?,
        },
        Subcommand::Mainnet { inner } => mainnet::go(opt, inner).await?,
        Subcommand::Util { inner } => inner.go(opt).await?,
    }

    Ok(())
}
