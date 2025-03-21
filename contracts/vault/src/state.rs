use crate::{prelude::*, types::WithdrawalRequest};
use cosmwasm_std::Storage;
use perpswap::contracts::vault::Config;

// Stores the global configuration of the vault
pub(crate) const CONFIG: Item<Config> = Item::new("config");

// Tracks pending withdrawals by user (key: address, value: amount)
pub(crate) const PENDING_WITHDRAWALS: Map<&str, Uint128> = Map::new("pending_withdrawals");

pub const TOTAL_PENDING_WITHDRAWALS: Item<Uint128> = Item::new("total_pending_withdrawals");

pub const WITHDRAWAL_QUEUE: Item<Vec<WithdrawalRequest>> = Item::new("withdrawal_queue");

// Total supply of USDCLP tokens issued
pub(crate) const TOTAL_LP_SUPPLY: Item<Uint128> = Item::new("total_lp_supply");

/// Amount of USDC initially allocated to each market (key: market address, value: amount).
/// Note: This represents the initial allocation and does not reflect the current value of LP
/// tokens, which may vary due to trader PnL, impermanent loss, or yield.
pub(crate) const MARKET_ALLOCATIONS: Map<&str, Uint128> = Map::new("market_allocations");

#[allow(dead_code)]
pub fn update<F>(storage: &mut dyn Storage, key: &str, f: F) -> StdResult<Uint128>
where
    F: FnOnce(Option<Uint128>) -> Result<Uint128, StdError>,
{
    MARKET_ALLOCATIONS.update(storage, key, f)
}
#[allow(dead_code)]
pub fn load(storage: &dyn Storage, key: &str) -> StdResult<Uint128> {
    MARKET_ALLOCATIONS.load(storage, key)
}
