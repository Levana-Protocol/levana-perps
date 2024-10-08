use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Contract, Wallet};
use msg::contracts::countertrade::HasWorkResp;
use perpswap::storage::MarketId;

use crate::{
    config::CounterTradeBotConfig,
    util::markets::Market,
    watcher::{ParallelWatcher, WatchedTaskOutput, WatchedTaskPerMarketParallel},
};

use super::{factory::FactoryInfo, App, AppBuilder};

pub(crate) struct CounterTradeBot {
    pub(crate) contract: Address,
}

impl AppBuilder {
    pub(super) fn start_countertrade_bot(&mut self, config: CounterTradeBotConfig) -> Result<()> {
        let bot = CounterTradeBot {
            contract: config.contract,
        };
        self.watch_periodic(
            crate::watcher::TaskLabel::CounterTradeBot,
            ParallelWatcher::new(bot),
        )
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
    match work {
        HasWorkResp::NoWork {} => Ok(WatchedTaskOutput::new("No work present")),
        HasWorkResp::Work { desc } => {
            let wallet = app.get_pool_wallet().await;
            do_countertrade_work(&contract, market_id, &wallet, &desc).await
        }
    }
}

async fn do_countertrade_work(
    contract: &Contract,
    market_id: MarketId,
    wallet: &Wallet,
    work: &msg::contracts::countertrade::WorkDescription,
) -> Result<WatchedTaskOutput> {
    let execute_msg = msg::contracts::countertrade::ExecuteMsg::DoWork { market: market_id };
    let response = contract.execute(wallet, vec![], execute_msg).await;
    match response {
        Ok(response) => Ok(WatchedTaskOutput::new(format!(
            "Successfully executed {work} in {}",
            response.txhash
        ))),
        Err(err) => Ok(WatchedTaskOutput::new(format!("Failed doing {work:?}: {err}")).set_error()),
    }
}
