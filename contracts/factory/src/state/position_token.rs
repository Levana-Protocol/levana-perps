use crate::state::*;
use anyhow::Result;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use msg::contracts::factory::entry::ContractType;
use perpswap::namespace;

use super::all_contracts::ALL_CONTRACTS;

/// Code ID of the position token contract
const POSITION_TOKEN_CODE_ID: Item<u64> = Item::new(namespace::POSITION_TOKEN_CODE_ID);

pub(super) const POSITION_TOKEN_ADDRS: Map<MarketId, Addr> =
    Map::new(namespace::POSITION_TOKEN_ADDRS);

pub(crate) fn position_token_code_id(store: &dyn Storage) -> Result<u64> {
    POSITION_TOKEN_CODE_ID.load(store).map_err(|err| err.into())
}

pub(crate) fn position_token_addr(store: &dyn Storage, market_id: MarketId) -> Result<Addr> {
    POSITION_TOKEN_ADDRS
        .load(store, market_id)
        .map_err(|err| err.into())
}

pub(crate) fn set_position_token_code_id(store: &mut dyn Storage, code_id: u64) -> Result<()> {
    POSITION_TOKEN_CODE_ID.save(store, &code_id)?;
    Ok(())
}

// save the position_token addr lookups
pub(crate) fn save_position_token_addr(
    store: &mut dyn Storage,
    market_id: MarketId,
    addr: &Addr,
) -> Result<()> {
    POSITION_TOKEN_ADDRS.save(store, market_id, addr)?;
    ALL_CONTRACTS.save(store, addr, &ContractType::PositionToken)?;
    Ok(())
}
