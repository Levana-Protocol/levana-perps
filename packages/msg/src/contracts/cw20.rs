#![allow(missing_docs)]
//! General CW20 contract messages.
//!
//! This is used by perps on testnet, not in production.
pub mod entry;
pub mod events;

use cosmwasm_std::{Addr, Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Cw20Coin {
    pub address: String,
    pub amount: Uint128,
}

impl Cw20Coin {
    pub fn is_empty(&self) -> bool {
        self.amount == Uint128::zero()
    }
}

impl fmt::Display for Cw20Coin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "address: {}, amount: {}", self.address, self.amount)
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Cw20CoinVerified {
    pub address: Addr,
    pub amount: Uint128,
}

impl Cw20CoinVerified {
    pub fn is_empty(&self) -> bool {
        self.amount == Uint128::zero()
    }
}

impl fmt::Display for Cw20CoinVerified {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "address: {}, amount: {}", self.address, self.amount)
    }
}

/// so that receivers of send messgage get the required encapsulation
#[derive(Serialize, Deserialize, Clone, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReceiverExecuteMsg {
    Receive(Cw20ReceiveMsg),
}

#[derive(Serialize, Deserialize, Clone, JsonSchema, Debug)]
pub struct Cw20ReceiveMsg {
    pub sender: String,
    pub amount: Uint128,
    pub msg: Binary,
}
