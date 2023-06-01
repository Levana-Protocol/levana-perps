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
            self.watch_periodic(TaskLabel::Stale, Stale)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Stale;

#[async_trait]
impl WatchedTaskPerMarket for Stale {
    async fn run_single_market(
        &mut self,
        app: &App,
        _factory: &FactoryInfo,
        _market: &MarketId,
        addr: Address,
    ) -> Result<WatchedTaskOutput> {
        check_stale_single(&app.cosmos, addr)
            .await
            .map(|message| WatchedTaskOutput {
                skip_delay: false,
                message,
            })
    }
}

async fn check_stale_single(cosmos: &Cosmos, addr: Address) -> Result<String> {
    let market = MarketContract::new(cosmos.make_contract(addr));
    let status = market.status().await?;
    let last_crank_completed = status
        .last_crank_completed
        .context("No cranks completed yet")?;
    let last_crank_completed = timestamp_to_date_time(last_crank_completed)?;
    let mk_message = |msg| Msg {
        msg,
        last_crank_completed,
        unpend_queue_size: status.unpend_queue_size,
        unpend_limit: status.config.unpend_limit,
    };
    if status.is_stale() {
        Err(mk_message("Protocol is in stale state").to_anyhow())
    } else if status.congested {
        Err(mk_message("Protocol is in congested state").to_anyhow())
    } else {
        Ok(mk_message("Protocol is neither stale nor congested").to_string())
    }
}

struct Msg<'a> {
    msg: &'a str,
    last_crank_completed: DateTime<Utc>,
    unpend_queue_size: u32,
    unpend_limit: u32,
}

impl Display for Msg<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Msg {
            msg,
            last_crank_completed,
            unpend_queue_size,
            unpend_limit,
        } = self;
        write!(f, "{msg}. Last completed crank timestamp: {last_crank_completed}. Unpend queue size: {unpend_queue_size}/{unpend_limit}.")
    }
}

impl Msg<'_> {
    fn to_anyhow(&self) -> anyhow::Error {
        anyhow!("{}", self)
    }
}
