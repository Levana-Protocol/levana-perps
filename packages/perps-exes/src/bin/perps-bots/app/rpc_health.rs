use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use chrono::Utc;

use crate::{
    app::factory::get_height,
    config::BotConfigMainnet,
    watcher::{Heartbeat, TaskLabel, WatchedTask, WatchedTaskOutput},
};

use super::{App, AppBuilder};

impl AppBuilder {
    pub(super) fn start_rpc_health(&mut self, mainnet: Arc<BotConfigMainnet>) -> Result<()> {
        self.watch_periodic(
            TaskLabel::RpcHealth,
            RpcHealth {
                endpoint: mainnet.rpc_endpoint.clone(),
            },
        )
    }
}

#[derive(Clone)]
struct RpcHealth {
    endpoint: Arc<String>,
}

#[async_trait]
impl WatchedTask for RpcHealth {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        check(&app, self.endpoint.clone()).await
    }
}

async fn check(app: &App, endpoint: Arc<String>) -> Result<WatchedTaskOutput> {
    let (endpoint, rpc_height) =
        get_height(endpoint, app.client.clone(), app.opt.referer_header.clone()).await?;
    let rpc_height: i64 = rpc_height.try_into()?;
    let grpc_latest = app.cosmos.get_latest_block_info().await?;
    let grpc_height = grpc_latest.height;
    let mut has_error = false;

    let mut msgs = vec![
        format!("RPC endpoint {endpoint} is showing block height {rpc_height}"),
        format!(
            "gRPC endpoint {} is showing block height {grpc_height}",
            app.cosmos.get_cosmos_builder().grpc_url()
        ),
    ];

    const ALLOWED_DELTA: u64 = 20;
    let delta = rpc_height.abs_diff(grpc_height);
    if delta <= ALLOWED_DELTA {
        msgs.push(format!(
            "Height delta {delta} is within allowed tolerance {ALLOWED_DELTA}"
        ));
    } else {
        has_error = true;
        msgs.push(format!(
            "Height delta {delta} is outside allowed tolerance {ALLOWED_DELTA}"
        ))
    }

    let age = Utc::now()
        .signed_duration_since(grpc_latest.timestamp)
        .num_seconds();
    const ALLOWED_AGE_SECONDS: i64 = 300;
    if age <= ALLOWED_AGE_SECONDS {
        msgs.push(format!(
            "Block age of {age} seconds is within allowed tolerance {ALLOWED_AGE_SECONDS}"
        ));
    } else {
        has_error = true;
        msgs.push(format!(
            "Block age of {age} seconds is outside allowed tolerance {ALLOWED_AGE_SECONDS}"
        ));
    }

    let msg = msgs.join("\n");
    if has_error {
        Err(anyhow::anyhow!("{msg}"))
    } else {
        Ok(WatchedTaskOutput::new(msg))
    }
}
