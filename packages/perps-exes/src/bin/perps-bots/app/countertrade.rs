use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Wallet};
use msg::contracts::countertrade::MarketsResp;

use crate::{
    config::CounterTradeBotConfig,
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
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
        self.watch_periodic(crate::watcher::TaskLabel::CounterTradeBot, bot)
    }
}

#[async_trait]
impl WatchedTask for CounterTradeBot {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        single_market(self, app).await
    }
}

async fn single_market(bot: &mut CounterTradeBot, app: Arc<App>) -> Result<WatchedTaskOutput> {
    let cosmos = app.cosmos.clone();
    let query = msg::contracts::countertrade::QueryMsg::Markets {
        start_after: None,
        limit: None,
    };
    let contract = cosmos.make_contract(bot.contract);
    let response: MarketsResp = contract.query(query).await?;
    let response = format!("{response:?}");
    Ok(WatchedTaskOutput::new(response))
}
