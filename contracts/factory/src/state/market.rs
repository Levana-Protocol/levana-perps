use crate::state::*;
use anyhow::Result;
use cosmwasm_std::Addr;
use cw_storage_plus::{Bound, Item, Map};
use msg::contracts::factory::entry::{ContractType, MarketsResp, MARKETS_QUERY_LIMIT_DEFAULT};
use shared::namespace;

use super::all_contracts::ALL_CONTRACTS;

/// Code ID of the market contract
const MARKET_CODE_ID: Item<u64> = Item::new(namespace::MARKET_CODE_ID);

/// Timestamp when market was added last for this factory
const MARKET_LAST_ADDED: Item<Option<Timestamp>> = Item::new(namespace::FACTORY_MARKET_LAST_ADDED);

/// The market addresses, keyed by market_id
pub(crate) const MARKET_ADDRS: Map<&MarketId, Addr> = Map::new(namespace::MARKET_ADDRS);

pub(crate) fn get_market_code_id(store: &dyn Storage) -> Result<u64> {
    MARKET_CODE_ID.load(store).map_err(|err| err.into())
}

pub(crate) fn markets(
    store: &dyn Storage,
    start_after: Option<MarketId>,
    limit: Option<u32>,
) -> Result<MarketsResp> {
    let limit = limit.unwrap_or(MARKETS_QUERY_LIMIT_DEFAULT);
    let markets = MARKET_ADDRS
        .keys(
            store,
            start_after.as_ref().map(Bound::exclusive),
            None,
            cosmwasm_std::Order::Ascending,
        )
        .take(limit.try_into()?)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MarketsResp { markets })
}

pub(crate) fn get_market_addr(store: &dyn Storage, market_id: &MarketId) -> Result<Addr> {
    MARKET_ADDRS
        .load(store, market_id)
        .map_err(|err| err.into())
}

pub(crate) fn set_market_code_id(store: &mut dyn Storage, code_id: u64) -> Result<()> {
    MARKET_CODE_ID.save(store, &code_id)?;
    Ok(())
}

// save the market addr lookups
pub(crate) fn save_market_addr(
    store: &mut dyn Storage,
    market_id: &MarketId,
    addr: &Addr,
    state: &State,
) -> Result<()> {
    MARKET_ADDRS.save(store, market_id, addr)?;
    ALL_CONTRACTS.save(store, addr, &ContractType::Market)?;
    MARKET_LAST_ADDED.save(store, &Some(Timestamp::from(state.env.block.time)))?;
    Ok(())
}
