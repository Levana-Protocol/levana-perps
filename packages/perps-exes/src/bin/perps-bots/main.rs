use anyhow::Result;
use clap::Parser;

mod app;
mod cli;
pub(crate) mod config;
mod endpoints;
mod util;
pub(crate) mod wallet_manager;
pub(crate) mod watcher;

#[tokio::main]
async fn main() -> Result<()> {
    main_inner().await
}

async fn main_inner() -> Result<()> {
    dotenv::dotenv().ok();

    let opt = cli::Opt::parse();
    opt.init_logger();
    opt.into_app_builder().await?.start().await
}
