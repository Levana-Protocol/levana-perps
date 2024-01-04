use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;

use crate::watcher::{Heartbeat, TaskLabel, WatchedTask, WatchedTaskOutput};

use super::{App, AppBuilder, OSMOSIS_MAX_GAS_PRICE};

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
    let base = app.cosmos.get_base_gas_price();
    if app.is_osmosis_congested() {
        Err(anyhow::anyhow!(
            "It appears that the Osmosis chain is congested. Current base gas price: {base}. Max allowed: {OSMOSIS_MAX_GAS_PRICE}",
        ))
    } else {
        Ok(WatchedTaskOutput::new(format!(
            "Chain does not appear to be congested. Current base gas price: {base}. Max allowed: {OSMOSIS_MAX_GAS_PRICE}"
        )))
    }
}
