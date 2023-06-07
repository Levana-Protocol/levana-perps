use anyhow::Result;
use axum::async_trait;
use chrono::{DateTime, Utc};
use cosmos::{Address, Cosmos};
use msg::prelude::*;
use perps_exes::{contracts::MarketContract, timestamp_to_date_time};

use crate::{
    config::BotConfigByType,
    watcher::{TaskLabel, WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, App, AppBuilder};

impl AppBuilder {
    pub(super) fn track_stale(&mut self) -> Result<()> {
        let ignore_stale = match &self.app.config.by_type {
            BotConfigByType::Testnet { inner } => inner.ignore_stale,
            BotConfigByType::Mainnet { .. } => false,
        };
        if !ignore_stale {
            self.watch_periodic(TaskLabel::Stale, Stale::default())?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
struct Stale {
    total_checks: u128,
    sum_of_unpends: u128,
    count_nonzero_unpend: u128,
}

#[async_trait]
impl WatchedTaskPerMarket for Stale {
    async fn run_single_market(
        &mut self,
        app: &App,
        _factory: &FactoryInfo,
        _market: &MarketId,
        addr: Address,
    ) -> Result<WatchedTaskOutput> {
        self.check_stale_single(&app.cosmos, addr)
            .await
            .map(|message| WatchedTaskOutput {
                skip_delay: false,
                message,
            })
    }
}

impl Stale {
    async fn check_stale_single(&mut self, cosmos: &Cosmos, addr: Address) -> Result<String> {
        let market = MarketContract::new(cosmos.make_contract(addr));
        let status = market.status().await?;
        let last_crank_completed = status
            .last_crank_completed
            .context("No cranks completed yet")?;
        let last_crank_completed = timestamp_to_date_time(last_crank_completed)?;

        self.total_checks += 1;
        self.sum_of_unpends += u128::from(status.unpend_queue_size);
        if status.unpend_queue_size > 0 {
            self.count_nonzero_unpend += 1;
        }

        let mk_message = |msg| Msg {
            msg,
            last_crank_completed,
            unpend_queue_size: status.unpend_queue_size,
            unpend_limit: status.config.unpend_limit,
            stale: self,
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
    stale: &'a Stale,
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
