use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;

use crate::watcher::{Heartbeat, TaskLabel, WatchedTask, WatchedTaskOutput};

use super::{App, AppBuilder};

impl AppBuilder {
    pub(super) fn start_congestion_alert(&mut self) -> Result<()> {
        self.watch_periodic(TaskLabel::Congestion, Congestion {})
    }
}

#[derive(Clone)]
struct Congestion {}

#[async_trait]
impl WatchedTask for Congestion {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        check(&app).await
    }
}

async fn check(app: &App) -> Result<WatchedTaskOutput> {
    let info = app.get_congested_info().await;
    if info.is_congested() {
        Err(anyhow::anyhow!(
            "It appears that the Osmosis chain is congested. {info:?}",
        ))
    } else {
        Ok(WatchedTaskOutput::new(format!(
            "Chain does not appear to be congested. {info:?}"
        )))
    }
}
