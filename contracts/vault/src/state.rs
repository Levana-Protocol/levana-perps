use crate::{prelude::*, types::WithdrawalRequest};
use perpswap::contracts::vault::Config;

pub(crate) const CONFIG: Item<Config> = Item::new("config");

pub const TOTAL_PENDING_WITHDRAWALS: Item<Uint128> = Item::new("total_pending_withdrawals");

pub const WITHDRAWAL_QUEUE: Item<Vec<WithdrawalRequest>> = Item::new("withdrawal_queue");

pub(crate) const TOTAL_LP_SUPPLY: Item<Uint128> = Item::new("total_lp_supply");

pub const LP_BALANCES: Map<&Addr, Uint128> = Map::new("lp_balances");

/// Amount of USDC initially allocated to each market (key: market address, value: amount).
/// Note: This represents the initial allocation and does not reflect the current value of LP
/// tokens, which may vary due to trader PnL, impermanent loss, or yield.
pub(crate) const MARKET_ALLOCATIONS: Map<&str, Uint128> = Map::new("market_allocations");
