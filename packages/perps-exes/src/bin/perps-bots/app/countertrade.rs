use std::str::FromStr;
use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use axum::async_trait;
use cosmos::{Address, Contract, Wallet};
use cosmwasm_std::Addr;
use perpswap::contracts::factory::entry::CounterTradeInfo;
use perpswap::contracts::market::entry::NewCounterTradeParams;
use perpswap::contracts::{countertrade::HasWorkResp, factory::entry::CounterTradeResp};
use perpswap::storage::MarketId;

use crate::{
    util::markets::Market,
    watcher::{ParallelWatcher, WatchedTaskOutput, WatchedTaskPerMarketParallel},
};

use super::{factory::FactoryInfo, App, AppBuilder};

pub(crate) struct CounterTradeBot;

impl AppBuilder {
    pub(super) fn start_countertrade_bot(&mut self) -> Result<()> {
        let bot = CounterTradeBot;
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
        factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        let factory = factory.factory;
        single_market(self, app, factory, market.market_id.clone()).await
    }
}

async fn single_market(
    _bot: Arc<CounterTradeBot>,
    app: &App,
    factory: Address,
    market_id: MarketId,
) -> Result<WatchedTaskOutput> {
    let cosmos = app.cosmos.clone();
    let countertrade = app.get_countertrade_contract(&market_id).await;
    let countertrade = match countertrade {
        Some(addr) => addr,
        None => {
            let factory = cosmos.make_contract(factory);
            let wallet = app.get_pool_wallet().await;
            let addr = create_countertrade_contract(&factory, market_id.clone(), &wallet).await?;
            let result = app
                .set_countertrade_contract(market_id.clone(), addr.clone())
                .await;
            assert_eq!(result, None, "Countertrade contract already exists");
            addr
        }
    };
    let countertrade = Address::from_str(countertrade.as_str())?;
    let query = perpswap::contracts::countertrade::QueryMsg::HasWork {};
    let contract = cosmos.make_contract(countertrade);
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
    _market_id: MarketId,
    wallet: &Wallet,
    work: &perpswap::contracts::countertrade::WorkDescription,
) -> Result<WatchedTaskOutput> {
    let execute_msg = perpswap::contracts::countertrade::ExecuteMsg::DoWork {};
    let response = contract.execute(wallet, vec![], execute_msg).await;
    match response {
        Ok(response) => Ok(WatchedTaskOutput::new(format!(
            "Successfully executed {work} in {}",
            response.txhash
        ))),
        Err(err) => Ok(WatchedTaskOutput::new(format!("Failed doing {work:?}: {err}")).set_error()),
    }
}

pub(crate) async fn get_countertrade_addresses(
    factory: &Contract,
) -> Result<HashMap<MarketId, Addr>> {
    let mut result = HashMap::new();
    let mut query_msg = perpswap::contracts::factory::entry::QueryMsg::CounterTrade {
        start_after: None,
        limit: None,
    };
    loop {
        let response: CounterTradeResp = factory.query(query_msg).await?;
        if response.addresses.is_empty() {
            break;
        }
        query_msg = perpswap::contracts::factory::entry::QueryMsg::CounterTrade {
            start_after: response.addresses.last().map(|item| item.market_id.clone()),
            limit: None,
        };
        for CounterTradeInfo {
            contract,
            market_id,
        } in response.addresses
        {
            result.insert(market_id, contract.0);
        }
    }
    Ok(result)
}

pub(crate) async fn create_countertrade_contract(
    factory: &Contract,
    market_id: MarketId,
    wallet: &Wallet,
) -> Result<Addr> {
    let execute_msg = perpswap::contracts::factory::entry::ExecuteMsg::AddCounterTrade {
        new_counter_trade: NewCounterTradeParams { market_id },
    };
    let response = factory.execute(wallet, vec![], execute_msg).await?;
    let addr = response
        .events
        .iter()
        .find(|e| e.r#type == "instantiate")
        .context("could not instantiate")?
        .attributes
        .iter()
        .find(|a| a.key == "_contract_address")
        .context("could not find contract_address")?
        .value
        .clone();
    Ok(Addr::unchecked(addr))
}
