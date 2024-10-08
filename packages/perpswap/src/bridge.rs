#![allow(missing_docs)]
use crate::contracts::market::entry::{ExecuteMsg, QueryMsg};
use crate::prelude::*;
use cosmwasm_std::{Addr, Binary, Event};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientToBridgeWrapper {
    pub msg_id: u64,
    pub user: Addr,
    pub msg: ClientToBridgeMsg,
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToBridgeMsg {
    QueryMarket {
        query_msg: QueryMsg,
    },
    ExecMarket {
        exec_msg: ExecuteMsg,
        funds: Option<NumberGtZero>,
    },
    RefreshPrice,
    Crank,
    MintCollateral {
        amount: NumberGtZero,
    },
    MintAndDepositLp {
        amount: NumberGtZero,
    },

    TimeJumpSeconds {
        seconds: i64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BridgeToClientWrapper {
    pub msg_id: u64,
    pub elapsed: f64,
    pub msg: BridgeToClientMsg,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BridgeToClientMsg {
    MarketQueryResult { result: Binary },
    MarketExecSuccess { events: Vec<Event> },
    MarketExecFailure(ExecError),
    TimeJumpResult {},
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ExecError {
    PerpError(PerpError),
    Unknown(String),
}
