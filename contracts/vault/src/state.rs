use crate::prelude::*;
use perpswap::contracts::vault::Config;

// Storage definitions

// Stores the global configuration of the vault
pub(crate) const CONFIG: Item<Config> = Item::new("config");

// Tracks pending withdrawals by user (key: address, value: amount)
pub(crate) const PENDING_WITHDRAWALS: Map<&str, Uint128> = Map::new("pending_withdrawals");

// Total supply of USDCLP tokens issued
pub(crate) const TOTAL_LP_SUPPLY: Item<Uint128> = Item::new("total_lp_supply");

// Amount of USDC allocated to each market (key: market address, value: amount)
pub(crate) const MARKET_ALLOCATIONS: Map<&str, Uint128> = Map::new("market_allocations");

// Indicates whether the contract is paused (true) or active (false)
pub(crate) const PAUSED: Item<bool> = Item::new("paused");
