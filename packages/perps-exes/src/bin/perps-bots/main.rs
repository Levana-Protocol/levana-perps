use anyhow::{Context, Result};
use clap::Parser;
use pid1::Pid1Settings;

mod app;
mod cli;
pub(crate) mod config;
mod endpoints;
mod util;
pub(crate) mod wallet_manager;
pub(crate) mod watcher;

#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
async fn main() -> Result<()> {
    main_inner().await
}

async fn main_inner() -> Result<()> {
    Pid1Settings::new().enable_log(true).launch()?;
    dotenv::dotenv().ok();

    let opt = cli::Opt::parse();
    opt.init_logger()?;

    let server = axum::Server::try_bind(&opt.bind)
        .with_context(|| format!("Cannot launch bot HTTP service bound to {}", opt.bind))?;

    opt.into_app_builder().await?.start(server).await
}
