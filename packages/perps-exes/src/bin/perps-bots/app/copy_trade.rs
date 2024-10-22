use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Contract, Wallet};
use perpswap::contracts::{
    copy_trading::{
        ExecuteMsg as CopyTradingExecuteMsg, QueryMsg as CopyTradingQueryMsg, WorkResp,
    },
    factory::entry::{CopyTradingInfoRaw, CopyTradingResp, QueryMsg as FactoryQueryMsg},
};

use crate::watcher::{
    ParallelCopyTradingWatcher, WatchedTaskOutput, WatchedTaskPerCopyTradingParallel,
};

use super::{
    factory::{CopyTrading, FactoryInfo},
    App, AppBuilder,
};

#[derive(Clone)]
pub(crate) struct CopyTradeBot;

impl AppBuilder {
    pub(super) fn start_copytrading_bot(&mut self) -> Result<()> {
        let watcher = ParallelCopyTradingWatcher::new(CopyTradeBot);
        self.watch_periodic(crate::watcher::TaskLabel::CopyTradeBot, watcher)
    }
}

pub(crate) async fn get_copy_trading_addresses(
    factory: &Contract,
    start_after: Option<CopyTradingInfoRaw>,
) -> Result<CopyTrading> {
    let mut result = vec![];
    let mut start_after = start_after;
    loop {
        let CopyTradingResp { addresses } =
            fetch_copy_trading_address(factory, start_after.clone()).await?;
        if addresses.is_empty() {
            break;
        }
        start_after = addresses.last().cloned().map(|item| CopyTradingInfoRaw {
            leader: item.leader.0.into(),
            contract: item.contract.0.into(),
        });
        for copy_trading_addr in addresses {
            let contract = copy_trading_addr.contract.0;
            let contract = contract.to_string().parse()?;
            result.push(contract);
        }
    }
    let result = CopyTrading {
        addresses: result,
        start_after,
        last_checked: Instant::now(),
    };
    Ok(result)
}

#[async_trait]
impl WatchedTaskPerCopyTradingParallel for CopyTradeBot {
    async fn run_single_copy_trading(
        self: Arc<Self>,
        app: &App,
        factory: &FactoryInfo,
        address: &Address,
    ) -> Result<WatchedTaskOutput> {
        let one_hour = 60 * 60;
        if factory.copy_trading.last_checked.elapsed() > Duration::from_secs(one_hour) {
            let mut copy_trading = factory.copy_trading.clone();
            let factory_contract = app.cosmos.make_contract(factory.factory);
            let remaining_copy_trading =
                get_copy_trading_addresses(&factory_contract, copy_trading.start_after.clone())
                    .await?;
            if !remaining_copy_trading.is_empty() {
                copy_trading.merge(remaining_copy_trading);
                let mut new_factory: FactoryInfo = factory.clone();
                new_factory.copy_trading = copy_trading;
                app.set_factory_info(new_factory).await;
            }
        }

        let copy_trading = address.to_string().parse()?;
        let contract = app.cosmos.make_contract(copy_trading);
        let wallet = app.get_pool_wallet().await;
        let response = do_all_copy_trading_work(&contract, &wallet).await?;
        match response.error {
            Some(error) => {
                let message = format!("{error}");
                Ok(WatchedTaskOutput::new(message).set_error())
            }
            None => Ok(WatchedTaskOutput::new(
                "Successfully finished executing all works",
            )),
        }
    }
}

async fn fetch_copy_trading_address(
    factory: &Contract,
    start_after: Option<CopyTradingInfoRaw>,
) -> Result<CopyTradingResp> {
    let query_msg = FactoryQueryMsg::CopyTrading {
        start_after,
        limit: None,
    };
    let response = factory.query(query_msg).await?;
    Ok(response)
}

pub(crate) struct ContractResponse {
    error: Option<cosmos::Error>,
}

async fn do_all_copy_trading_work(
    contract: &Contract,
    wallet: &Wallet,
) -> Result<ContractResponse> {
    let query_msg = CopyTradingQueryMsg::HasWork {};
    let execute_msg = CopyTradingExecuteMsg::DoWork {};
    loop {
        let work: WorkResp = contract.query(&query_msg).await?;
        if work.has_work() {
            let response = contract.execute(wallet, vec![], &execute_msg).await;
            match response {
                Ok(_) => continue,
                Err(error) => return Ok(ContractResponse { error: Some(error) }),
            }
        } else {
            return Ok(ContractResponse { error: None });
        }
    }
}
