pub(crate) use anyhow::{anyhow, ensure, Context, Result};
pub(crate) use cosmwasm_std::{
    entry_point, from_json, to_json_binary, Addr, Api, Binary, Coin, Decimal256, Deps, DepsMut,
    Empty, Env, Event, MessageInfo, Order, QuerierWrapper, Response, Storage, Uint128,
};
pub(crate) use cw_storage_plus::{Bound, Item, Map};
pub(crate) use perpswap::contracts::copy_trading::*;
pub(crate) use perpswap::contracts::market::entry::QueryMsg as MarketQueryMsg;
pub(crate) use perpswap::{
    attr_map,
    storage::{Collateral, LpToken, MarketId, NonZero, Signed, UnsignedDecimal},
};

/// Perform sanity checks in dev, no-op in prod.
#[cfg(debug_assertions)]
pub use crate::sanity::sanity;

#[cfg(not(debug_assertions))]
pub fn sanity(_: &dyn Storage, _: &Env) {}
