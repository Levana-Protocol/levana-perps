use std::{fmt::Write, sync::Arc};

use anyhow::Result;
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
    let mut is_healthy = false;
    let mut res = String::new();
    for report in app.cosmos.node_health_report().nodes {
        match report.node_health_level {
            cosmos::error::NodeHealthLevel::Unblocked { error_count } if error_count < 4 => {
                writeln!(&mut res, "Healthy: {report}")?;
                is_healthy = true;
            }
            _ => {
                writeln!(&mut res, "Unhealthy: {report}")?;
            }
        }
    }
    if is_healthy {
        Ok(res)
    } else {
        Err(anyhow::anyhow!(res))
    }
}
