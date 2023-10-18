use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;

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
    let (endpoint, rpc_height) = get_height(endpoint, app.client.clone()).await?;
    let rpc_height: i64 = rpc_height.try_into()?;
    let grpc_height = app.cosmos.get_latest_block_info().await?.height;

    const ALLOWED_DELTA: u64 = 20;

    let delta = rpc_height.abs_diff(grpc_height);

    if delta < ALLOWED_DELTA {
        Ok(WatchedTaskOutput {
            skip_delay: false,
            message: format!("RPC endpoint {endpoint} looks healthy. Delta: {delta}. RPC height: {rpc_height}. gRPC height: {grpc_height}.")
        })
    } else {
        Err(anyhow::anyhow!("RPC endpoint {endpoint} has too high a block height delta {delta}. RPC height: {rpc_height}. gRPC height: {grpc_height}."))
    }
}
