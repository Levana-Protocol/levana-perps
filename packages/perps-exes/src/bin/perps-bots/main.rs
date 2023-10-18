use std::net::SocketAddr;

use anyhow::{bail, Context, Result};
use clap::Parser;
use cli::Opt;
use pid1::Pid1Settings;
use sentry::{IntoDsn, ClientInitGuard};

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

    let sentry_guard = if let Some(sentry_dsn) = opt.client_key.clone() {
        let guard = sentry::init((
            sentry_dsn,
            sentry::ClientOptions {
                release: sentry::release_name!(),
                // Have it as 0. in prod
                sample_rate: 1.0,
                traces_sample_rate: 1.0,
                session_mode: sentry::SessionMode::Request,
                debug: true,
                ..Default::default()
            },
        ));
        Some(guard)
    } else {
        None
    };

    opt.init_logger(&sentry_guard)?;

    let addr = opt.bind.clone();

    tokio_spwan(sentry_guard, addr, opt)
}

fn tokio_spwan(sentry_guard: Option<ClientInitGuard>, addr: SocketAddr, opt: Opt) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(16)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            opt.into_app_builder(sentry_guard, addr).await
            // app.start(addr).await

        })
        .map_err(anyhow::Error::msg)
}
