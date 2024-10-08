use crate::state::*;
use anyhow::Result;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use perpswap::contracts::{factory::entry::ContractType, liquidity_token::LiquidityTokenKind};
use perpswap::namespace;

use super::all_contracts::ALL_CONTRACTS;

/// Code ID of the liquidity token contract
const LIQUIDITY_TOKEN_CODE_ID: Item<u64> = Item::new(namespace::LIQUIDITY_TOKEN_CODE_ID);

pub(super) const LP_ADDRS: Map<MarketId, Addr> = Map::new(namespace::LP_ADDRS);
pub(super) const XLP_ADDRS: Map<MarketId, Addr> = Map::new(namespace::XLP_ADDRS);
pub(super) const LP_ADDRS_REVERSE: Map<&Addr, MarketId> = Map::new(namespace::LP_ADDRS_REVERSE);
pub(super) const XLP_ADDRS_REVERSE: Map<&Addr, MarketId> = Map::new(namespace::XLP_ADDRS_REVERSE);

pub(crate) fn liquidity_token_code_id(store: &dyn Storage) -> Result<u64> {
    LIQUIDITY_TOKEN_CODE_ID
        .load(store)
        .map_err(|err| err.into())
}

pub(crate) fn liquidity_token_addr(
    store: &dyn Storage,
    market_id: MarketId,
    kind: LiquidityTokenKind,
) -> Result<Addr> {
    addrs_map(kind)
        .load(store, market_id)
        .map_err(|err| err.into())
}

pub(crate) fn set_liquidity_token_code_id(store: &mut dyn Storage, code_id: u64) -> Result<()> {
    LIQUIDITY_TOKEN_CODE_ID.save(store, &code_id)?;
    Ok(())
}

// save the liquidity_token addr lookups
pub(crate) fn save_liquidity_token_addr(
    store: &mut dyn Storage,
    market_id: MarketId,
    addr: &Addr,
    kind: LiquidityTokenKind,
) -> Result<()> {
    if addrs_map(kind)
        .may_load(store, market_id.clone())?
        .is_some()
    {
        perp_bail!(
            ErrorId::AddressAlreadyExists,
            ErrorDomain::Factory,
            "liquidity token address for market {} already exists",
            market_id
        );
    }

    addrs_map(kind).save(store, market_id.clone(), addr)?;
    addrs_map_rev(kind).save(store, addr, &market_id)?;
    ALL_CONTRACTS.save(store, addr, &ContractType::LiquidityToken)?;

    Ok(())
}

pub(crate) fn addrs_map(kind: LiquidityTokenKind) -> Map<MarketId, Addr> {
    match kind {
        LiquidityTokenKind::Lp => LP_ADDRS,
        LiquidityTokenKind::Xlp => XLP_ADDRS,
    }
}
pub(crate) fn addrs_map_rev(kind: LiquidityTokenKind) -> Map<&'static Addr, MarketId> {
    match kind {
        LiquidityTokenKind::Lp => LP_ADDRS_REVERSE,
        LiquidityTokenKind::Xlp => XLP_ADDRS_REVERSE,
    }
}
