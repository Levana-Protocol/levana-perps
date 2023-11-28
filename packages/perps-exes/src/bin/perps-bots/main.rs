use anyhow::{Context, Result};
use clap::Parser;
use pid1::Pid1Settings;
use tokio::net::TcpListener;

mod app;
mod cli;
pub(crate) mod config;
mod endpoints;
mod util;
pub(crate) mod wallet_manager;
pub(crate) mod watcher;

fn main() -> Result<()> {
    Pid1Settings::new().enable_log(true).launch()?;
    main_inner()
}

fn main_inner() -> Result<()> {
    dotenv::dotenv().ok();

    let opt = cli::Opt::parse();

    opt.init_logger()?;
    let _guard = opt.sentry_dsn.clone().map(|sentry_dsn| {
        sentry::init((
            sentry_dsn,
            sentry::ClientOptions {
                release: sentry::release_name!(),
                session_mode: sentry::SessionMode::Application,
                debug: false,
                // Have 1% sampling rate at production
                traces_sample_rate: 0.01,
                ..Default::default()
            },
        ))
    });

    // We do not use tokio macro because of this:
    // https://docs.sentry.io/platforms/rust/#async-main-function
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(16)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let listener = TcpListener::bind(&opt.bind).await.context(format!(
                "Cannot launch bot HTTP service bound to {}",
                opt.bind
            ))?;
            opt.into_app_builder().await?.start(listener).await
        })
        .map_err(anyhow::Error::msg)
}
