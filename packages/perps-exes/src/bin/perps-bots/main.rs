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
    let _guard = opt.client_key.clone().map(|ck| {
        sentry::init((
            ck,
            sentry::ClientOptions {
                release: sentry::release_name!(),
                session_mode: sentry::SessionMode::Request,
                ..Default::default()
            },
        ))
    });
    opt.into_app_builder().await?.start().await
}
