use std::{fmt::Display, sync::Arc};

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Contract, HasAddress, Wallet};
use perpswap::contracts::{
    copy_trading::{
        ExecuteMsg as CopyTradingExecuteMsg, QueryMsg as CopyTradingQueryMsg, WorkResp,
    },
    factory::entry::{CopyTradingInfoRaw, CopyTradingResp, QueryMsg as FactoryQueryMsg},
};

use crate::watcher::{Heartbeat, WatchedTask, WatchedTaskOutput};

use super::{App, AppBuilder};

#[derive(Clone)]
pub(crate) struct CopyTradeBot;

impl AppBuilder {
    pub(super) fn start_copytrading_bot(&mut self) -> Result<()> {
        self.watch_periodic(crate::watcher::TaskLabel::CopyTradeBot, CopyTradeBot)
    }
}

#[async_trait]
impl WatchedTask for CopyTradeBot {
    async fn run_single(
        &mut self,
        app: Arc<App>,
        _heartbeat: Heartbeat,
    ) -> Result<WatchedTaskOutput> {
        let factory = app.get_factory_info().await;
        let factory = factory.factory;
        let cosmos = app.cosmos.clone();
        let factory_contract = cosmos.make_contract(factory);
        let mut start_after = None;
        let mut result = vec![];
        let mut total_contracts = 0;
        loop {
            let CopyTradingResp { addresses } =
                fetch_copy_trading_address(&factory_contract, start_after.clone()).await?;
            if addresses.is_empty() {
                break;
            }
            start_after = addresses.last().cloned().map(|item| CopyTradingInfoRaw {
                leader: item.leader.0.into(),
                contract: item.contract.0.into(),
            });
            for copy_trading_addr in addresses {
                total_contracts += 1;
                let contract = copy_trading_addr.contract.0;
                let contract = contract.to_string().parse()?;
                let copy_trading_contract = cosmos.make_contract(contract);
                let wallet = app.get_pool_wallet().await;
                let response = do_all_copy_trading_work(&copy_trading_contract, &wallet).await?;
                if response.is_err() {
                    result.push(response);
                }
            }
        }
        if result.is_empty() {
            let msg = format!("Successfully finished executing a single round (Total contracts: {total_contracts})");
            Ok(WatchedTaskOutput::new(msg))
        } else {
            let mut msg = String::new();
            for error in result {
                msg.push_str(&format!("{error}"));
            }
            Ok(WatchedTaskOutput::new(msg).set_error())
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
