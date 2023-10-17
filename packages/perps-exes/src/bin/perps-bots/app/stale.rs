use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use chrono::{DateTime, Utc};
use cosmos::{Address, HasAddress};
use dashmap::DashMap;
use msg::prelude::*;
use perps_exes::{contracts::MarketContract, timestamp_to_date_time};

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
struct Stale {
    markets: Arc<DashMap<Address, StaleMarket>>,
}

#[derive(Clone, Copy, Default)]
struct StaleMarket {
    total_checks: u128,
    sum_of_unpends: u128,
    count_nonzero_unpend: u128,
}

#[async_trait]
impl WatchedTaskPerMarketParallel for Stale {
    async fn run_single_market(
        self: Arc<Self>,
        _app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        self.check_stale_single(&market.market)
            .await
            .map(|message| WatchedTaskOutput {
                skip_delay: false,
                message,
            })
    }
}

impl Stale {
    async fn check_stale_single(&self, market: &MarketContract) -> Result<String> {
        let status = market.status().await?;
        let last_crank_completed = status
            .last_crank_completed
            .context("No cranks completed yet")?;
        let last_crank_completed = timestamp_to_date_time(last_crank_completed)?;

        let address = market.get_address();
        let mut stats = self
            .markets
            .get(&address)
            .map_or_else(StaleMarket::default, |x| *x);
        stats.total_checks += 1;
        stats.sum_of_unpends += u128::from(status.unpend_queue_size);
        if status.unpend_queue_size > 0 {
            stats.count_nonzero_unpend += 1;
        }
        self.markets.insert(address, stats);

        let mk_message = |msg| Msg {
            msg,
            last_crank_completed,
            unpend_queue_size: status.unpend_queue_size,
            unpend_limit: status.config.unpend_limit,
            stale: &stats,
        };
        if status.is_stale() {
            Err(mk_message("Protocol is in stale state").to_anyhow())
        } else if status.congested {
            Err(mk_message("Protocol is in congested state").to_anyhow())
        } else {
            Ok(mk_message("Protocol is neither stale nor congested").to_string())
        }
    }
}

struct Msg<'a> {
    msg: &'a str,
    last_crank_completed: DateTime<Utc>,
    unpend_queue_size: u32,
    unpend_limit: u32,
    stale: &'a StaleMarket,
}

impl Display for Msg<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Msg {
            msg,
            last_crank_completed,
            unpend_queue_size,
            unpend_limit,
            stale,
        } = self;
        let average = if stale.total_checks == 0 {
            Decimal256::zero()
        } else {
            Decimal256::from_ratio(stale.sum_of_unpends, stale.total_checks)
        };
        write!(f, "{msg}. Last completed crank timestamp: {last_crank_completed}. Unpend queue size: {unpend_queue_size}/{unpend_limit}. Statuses with non-zero unpend: {}/{}. Average size: {average}.", stale.count_nonzero_unpend, stale.total_checks)
    }
}

impl Msg<'_> {
    fn to_anyhow(&self) -> anyhow::Error {
        anyhow!("{}", self)
    }
}
