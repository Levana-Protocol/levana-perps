use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Contract};
use msg::contracts::market;
use perps_exes::prelude::MarketId;

use crate::watcher::{WatchedTaskOutput, WatchedTaskPerMarket};

use super::factory::FactoryInfo;
use super::{App, AppBuilder};

#[derive(Clone)]
struct Worker {}

/// Start the background thread to turn the crank on the crank bots.
impl AppBuilder {
    pub(super) async fn start_crank_bot(&mut self) -> Result<()> {
        self.refill_gas(*self.app.config.crank_wallet.address(), "crank-bot")?;

        let worker = Worker {};
        self.watch_periodic(crate::watcher::TaskLabel::Crank, worker)
    }
}

#[async_trait]
impl WatchedTaskPerMarket for Worker {
    async fn run_single_market(
        &self,
        app: &App,
        _factory: &FactoryInfo,
        _market: &MarketId,
        addr: Address,
    ) -> Result<WatchedTaskOutput> {
        app.crank(addr).await
    }
}

impl App {
    async fn crank(&self, addr: Address) -> Result<WatchedTaskOutput> {
        let market = self.cosmos.make_contract(addr);
        let work = match self.check_crank(&market).await? {
            None => {
                return Ok(WatchedTaskOutput {
                    skip_delay: false,
                    message: "No crank messages waiting".to_owned(),
                })
            }
            Some(work) => work,
        };

        let txres = self
            .cosmos
            .make_contract(addr)
            .execute(
                &self.config.crank_wallet,
                vec![],
                market::entry::ExecuteMsg::Crank {
                    execs: Some(40),
                    rewards: None,
                },
            )
            .await?;
        Ok(WatchedTaskOutput {
            skip_delay: true,
            message: format!(
                "Successfully turned the crank for work item {work:?} in transaction {}",
                txres.txhash
            ),
        })
    }

    async fn check_crank(&self, market: &Contract) -> Result<Option<market::crank::CrankWorkInfo>> {
        let market::entry::StatusResp { next_crank, .. } =
            market.query(market::entry::QueryMsg::Status {}).await?;

        Ok(next_crank)
    }
}
