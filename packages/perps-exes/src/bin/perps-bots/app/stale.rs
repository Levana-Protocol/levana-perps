use std::{sync::Arc, time::Instant};

use anyhow::Result;
use axum::async_trait;
use chrono::Utc;
use cosmos::HasAddress;
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

        let next_deferred = match status.next_deferred_execution {
            Some(next_deferred) => next_deferred.try_into_chrono_datetime()?,
            None => return Ok("No deferred execution items found, we're not stale".to_owned()),
        };

        let age = Utc::now().signed_duration_since(next_deferred);
        if age.num_minutes() < 5 {
            return Ok("Oldest deferred execution item is less than 5 minutes old".to_owned());
        }

        if app.is_osmosis_epoch() {
            return Ok(
                "Ignoring old deferred exec item since we're in the Osmosis epoch".to_owned(),
            );
        }

        // TODO in the future, we'd like to give a grace period after the
        // markets reopen for deferred exec items to catch up. Waiting until we start
        // seeing spurious errors on mainnet to address this.
        if app
            .pyth_prices_closed(market.get_address(), Some(&status))
            .await?
        {
            return Ok("Ignoring old deferred exec item since we're the market is in off hours for price updates".to_owned());
        }

        Err(anyhow::anyhow!(
            "Oldest pending deferred exec item is too old ({age})"
        ))
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
