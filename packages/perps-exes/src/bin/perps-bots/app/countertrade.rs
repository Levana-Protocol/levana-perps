use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Contract, Wallet};
use msg::contracts::countertrade::HasWorkResp;
use shared::storage::MarketId;

use crate::{
    config::CounterTradeBotConfig,
    util::markets::Market,
    watcher::{ParallelWatcher, WatchedTaskOutput, WatchedTaskPerMarketParallel},
};

use super::{factory::FactoryInfo, App, AppBuilder};

pub(crate) struct CounterTradeBot {
    pub(crate) wallet: Arc<Wallet>,
    pub(crate) contract: Address,
}

impl AppBuilder {
    pub(super) fn start_countertrade_bot(&mut self, config: CounterTradeBotConfig) -> Result<()> {
        let bot = CounterTradeBot {
            wallet: config.wallet,
            contract: config.contract,
        };
        self.watch_periodic(crate::watcher::TaskLabel::CounterTradeBot, ParallelWatcher::new(bot))
    }
}

#[async_trait]
impl WatchedTaskPerMarketParallel for CounterTradeBot {
    async fn run_single_market(
        self: Arc<Self>,
        app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        single_market(self, app, market.market_id.clone()).await
    }
}

async fn single_market(
    bot: Arc<CounterTradeBot>,
    app: &App,
    market_id: MarketId,
) -> Result<WatchedTaskOutput> {
    let cosmos = app.cosmos.clone();
    let query = msg::contracts::countertrade::QueryMsg::HasWork {
        market: market_id.clone(),
    };
    let contract = cosmos.make_contract(bot.contract);
    let work: HasWorkResp = contract.query(query).await?;
    let wallet = bot.wallet.clone();
    match work {
        HasWorkResp::NoWork {} => Ok(WatchedTaskOutput::new("No work present")),
        HasWorkResp::Work { desc } => match desc {
            msg::contracts::countertrade::WorkDescription::OpenPosition { .. } => {
                do_countertrade_work(&contract, market_id, &wallet, &Action::Open).await
            }
            msg::contracts::countertrade::WorkDescription::ClosePosition { .. } => {
                do_countertrade_work(&contract, market_id, &wallet, &Action::Close).await
            }
            msg::contracts::countertrade::WorkDescription::CollectClosedPosition { .. } => {
                do_countertrade_work(&contract, market_id, &wallet, &Action::CollectClosed).await
            }
            msg::contracts::countertrade::WorkDescription::ResetShares => {
                do_countertrade_work(&contract, market_id, &wallet, &Action::ResetShares).await
            }
            msg::contracts::countertrade::WorkDescription::ClearDeferredExec { .. } => {
                do_countertrade_work(&contract, market_id, &wallet, &Action::ClearDeferred).await
            }
        },
    }
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Debug)]
enum Action {
    Open,
    Close,
    CollectClosed,
    ResetShares,
    ClearDeferred,
}

async fn do_countertrade_work(
    contract: &Contract,
    market_id: MarketId,
    wallet: &Wallet,
    action: &Action,
) -> Result<WatchedTaskOutput> {
    let execute_msg = msg::contracts::countertrade::ExecuteMsg::DoWork { market: market_id };
    let response = contract.execute(&wallet, vec![], execute_msg).await;
    match response {
        Ok(response) => Ok(WatchedTaskOutput::new(format!(
            "Succesfully exected {action:?} in {}",
            response.txhash
        ))),
        Err(err) => {
            Ok(WatchedTaskOutput::new(format!("Failed doing {action:?}: {err}")).set_error())
        }
    }
}
