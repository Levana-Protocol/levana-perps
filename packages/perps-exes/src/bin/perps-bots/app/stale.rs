use std::{sync::Arc, time::Instant};

use anyhow::Result;
use axum::async_trait;
use chrono::{DateTime, Utc};
use msg::prelude::*;
use perps_exes::contracts::MarketContract;

use crate::{
    config::BotConfigByType,
    util::markets::Market,
    watcher::{ParallelWatcher, TaskLabel, WatchedTaskOutput, WatchedTaskPerMarketParallel},
};

use super::{factory::FactoryInfo, App, AppBuilder};

impl AppBuilder {
    pub(super) fn track_stale(&mut self) -> Result<()> {
        let ignore_stale = match &self.app.config.by_type {
            BotConfigByType::Testnet { inner } => inner.ignore_stale,
            BotConfigByType::Mainnet { .. } => false,
        };
        if !ignore_stale {
            self.watch_periodic(TaskLabel::Stale, ParallelWatcher::new(Stale::default()))?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Stale {}

#[async_trait]
impl WatchedTaskPerMarketParallel for Stale {
    async fn run_single_market(
        self: Arc<Self>,
        app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        self.check_stale_single(&market.market, app)
            .await
            .map(WatchedTaskOutput::new)
    }
}

impl Stale {
    async fn check_stale_single(&self, market: &MarketContract, app: &App) -> Result<String> {
        let status = market.status().await?;
        let last_crank_completed = status
            .last_crank_completed
            .context("No cranks completed yet")?
            .try_into_chrono_datetime()?;

        let mk_message = |msg| Msg {
            msg,
            last_crank_completed,
            deferred_execution_items: status.deferred_execution_items,
        };
        let age = Utc::now().signed_duration_since(last_crank_completed);
        if age > chrono::Duration::seconds(MAX_ALLOWED_CRANK_AGE_SECS) {
            if app.is_osmosis_epoch() {
                Ok(mk_message(&format!("Last crank is too old (not run since {last_crank_completed}, age is {age}), but we think we're in an Osmosis epoch so ignoring")).to_string())
            } else {
                Err(mk_message(&format!(
                    "Crank has not been run since {last_crank_completed}, age of {age} is too high"
                ))
                .to_anyhow())
            }
        } else {
            Ok(mk_message("Protocol is neither stale nor congested").to_string())
        }
    }
}

// This should be at least 60 seconds more than MAX_CRANK_AGE in crank_watch to avoid spurious
// errors
const MAX_ALLOWED_CRANK_AGE_SECS: i64 = 300;

struct Msg<'a> {
    msg: &'a str,
    last_crank_completed: DateTime<Utc>,
    deferred_execution_items: u32,
}

impl Display for Msg<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Msg {
            msg,
            last_crank_completed,
            deferred_execution_items,
        } = self;
        write!(f, "{msg}. Last completed crank timestamp: {last_crank_completed}. Deferred execution queue size: {deferred_execution_items}.")
    }
}

impl Msg<'_> {
    fn to_anyhow(&self) -> anyhow::Error {
        anyhow!("{}", self)
    }
}

impl App {
    /// Do we think we're in the middle of an Osmosis epoch?
    pub(crate) fn is_osmosis_epoch(&self) -> bool {
        let mut guard = self.epoch_last_seen.lock();

        if self.cosmos.is_chain_paused() {
            *guard = Some(Instant::now());
            return true;
        }

        let epoch_last_seen = match *guard {
            None => return false,
            Some(epoch_last_seen) => epoch_last_seen,
        };

        let now = Instant::now();
        let age = match now.checked_duration_since(epoch_last_seen) {
            None => {
                tracing::warn!("is_osmosis_epoch: checked_duration_since returned a None");
                return false;
            }
            Some(age) => age,
        };

        if age.as_secs() > self.config.ignore_errors_after_epoch_seconds.into() {
            // Happened too long ago, so we're not in the epoch anymore
            *guard = None;
            false
        } else {
            true
        }
    }
}
