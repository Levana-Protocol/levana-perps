use crate::{prelude::*, types::WithdrawalRequest};
use cosmwasm_std::Uint64;
use cw_storage_plus::{Key, KeyDeserialize, PrimaryKey};
use perpswap::contracts::vault::Config;
use serde::{Deserialize, Serialize};

pub const CONFIG: Item<Config> = Item::new("config");

pub const TOTAL_PENDING_WITHDRAWALS: Item<Uint128> = Item::new("total_pending_withdrawals");

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Debug)]
#[serde(transparent)]
pub struct QueueId(pub Uint64);

impl PrimaryKey<'_> for QueueId {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.u64().to_be_bytes())]
    }
}

impl KeyDeserialize for QueueId {
    type Output = Self;

    fn from_vec(value: Vec<u8>) -> Result<Self, StdError> {
        let bytes: [u8; 8] = value
            .try_into()
            .map_err(|_| StdError::parse_err("Uint64", "Input length must be 8 bytes"))?;
        let num = u64::from_be_bytes(bytes);
        Ok(QueueId(Uint64::new(num)))
    }

    const KEY_ELEMS: u16 = 1;
}

pub const QUEUE_ID: Item<QueueId> = Item::new("queue_id");

pub const USER_WITHDRAWALS: Map<(&Addr, QueueId), ()> = Map::new("user_withdrawals");

pub const WITHDRAWAL_QUEUE: Map<QueueId, WithdrawalRequest> = Map::new("withdrawal_queue");

pub const TOTAL_LP_SUPPLY: Item<Uint128> = Item::new("total_lp_supply");

pub const LP_BALANCES: Map<&Addr, Uint128> = Map::new("lp_balances");

// It will be  a limit of 50 for now
pub const MARKET_ALLOCATIONS: Map<&str, Uint128> = Map::new("market_allocations");
