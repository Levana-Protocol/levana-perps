#![allow(dead_code)]
#![allow(unused_variables)]
use std::{cell::RefCell, rc::Rc};

use cosmwasm_std::{
    entry_point, from_json, Binary, Deps, DepsMut, Env, MessageInfo, QueryResponse, Response,
    Uint128,
};
use cw_multi_test::Executor;
use perpswap::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{config::TEST_CONFIG, LocalContractWrapper, PerpsApp};

pub struct MockCw20ReceiverContract {
    app: Rc<RefCell<PerpsApp>>,
    pub addr: Addr,
    pub code_id: u64,
}

impl MockCw20ReceiverContract {
    pub fn new(app: Rc<RefCell<PerpsApp>>) -> Result<Self> {
        let contract = Box::new(LocalContractWrapper::new(instantiate, execute, query));

        let code_id = app.borrow_mut().store_code(contract);

        let addr = app.borrow_mut().instantiate_contract(
            code_id,
            Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &InstantiateMsg {},
            &[],
            "factory",
            Some(TEST_CONFIG.migration_admin.clone()),
        )?;

        Ok(Self { app, code_id, addr })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {}

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response> {
    Ok(Response::default())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive {
        sender: String,
        amount: Uint128,
        msg: Binary,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Payload {
    Print {
        value: String,
        enforce_sender: Option<String>,
        enforce_info_sender: Option<String>,
    },
}

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    match msg {
        ExecuteMsg::Receive {
            sender,
            amount,
            msg,
        } => match from_json::<Payload>(&msg)? {
            Payload::Print {
                value,
                enforce_sender,
                enforce_info_sender,
            } => {
                if let Some(enforce_sender) = enforce_sender {
                    if enforce_sender != sender {
                        bail!(
                            "invalid sender, expected: {}, got: {}",
                            enforce_sender,
                            sender
                        );
                    }
                }
                if let Some(enforce_info_sender) = enforce_info_sender {
                    if enforce_info_sender != info.sender.as_str() {
                        bail!(
                            "invalid sender, expected: {}, got: {}",
                            enforce_info_sender,
                            info.sender
                        );
                    }
                }
                println!(
                    "Receive: sender: {}, amount: {}, value: {}",
                    sender, amount, value
                );
            }
        },
    }
    Ok(Response::default())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct QueryMsg {}
#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    Ok(QueryResponse::default())
}
