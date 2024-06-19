use std::sync::Arc;

use anyhow::{Context, Result};
use axum::async_trait;

use crate::watcher::{Heartbeat, TaskLabel, WatchedTask, WatchedTaskOutput};

use super::{App, AppBuilder};

impl AppBuilder {
    pub(super) fn track_block_lag(&mut self) -> Result<()> {
        self.watch_periodic(TaskLabel::BlockLag, BlockLag)?;
        Ok(())
    }
}

#[derive(Default)]
struct BlockLag;

#[async_trait]
impl WatchedTask for BlockLag {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        check_block_lag_single(&app)
            .await
            .map(WatchedTaskOutput::new)
    }
}

async fn check_block_lag_single(app: &App) -> Result<String> {
    let report = app
        .cosmos
        .node_health_report()
        .nodes
        .into_iter()
        .next()
        .context("Impossible! No nodes found in health report")?;
    match report.node_health_level {
        cosmos::error::NodeHealthLevel::Unblocked { error_count } if error_count < 4 => {
            Ok(format!("Primary node is healthy:\n{report}"))
        }
        _ => Err(anyhow::anyhow!("Primary node is not healthy:\n{report}")),
    }
}
