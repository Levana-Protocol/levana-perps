use std::{
    fmt::Display,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Contract, HasAddress, Wallet};
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
            fetch_copy_trading_address(&factory, start_after.clone()).await?;
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
        if response.is_err() {
            let mut msg = String::new();
            for error in response.errors {
                msg.push_str(&format!("{error}"));
            }
            Ok(WatchedTaskOutput::new(msg).set_error())
        } else {
            let msg = format!("Successfully finished executing all works");
            Ok(WatchedTaskOutput::new(msg))
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
    is_error: bool,
    contract: Address,
    errors: Vec<cosmos::Error>,
}

impl Display for ContractResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Contract {} \n errors: {:?}", self.contract, self.errors)
    }
}

impl ContractResponse {
    pub fn is_err(&self) -> bool {
        self.is_error
    }
}

async fn do_all_copy_trading_work(
    contract: &Contract,
    wallet: &Wallet,
) -> Result<ContractResponse> {
    let query_msg = CopyTradingQueryMsg::HasWork {};
    let execute_msg = CopyTradingExecuteMsg::DoWork {};
    let mut is_error = false;
    let mut errors = vec![];
    loop {
        let work: WorkResp = contract.query(&query_msg).await?;
        if work.has_work() {
            let response = contract.execute(wallet, vec![], &execute_msg).await;
            if let Err(err) = response {
                is_error = true;
                errors.push(err);
                break;
            }
        } else {
            break;
        }
    }
    Ok(ContractResponse {
        is_error,
        errors,
        contract: contract.get_address(),
    })
}
