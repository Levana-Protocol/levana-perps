//! Tracks all contracts that are part of this factory for efficient query purposes.

use cosmwasm_std::Addr;
use cw_storage_plus::Map;
use msg::contracts::factory::entry::ContractType;
use perpswap::namespace;

pub(crate) const ALL_CONTRACTS: Map<&Addr, ContractType> = Map::new(namespace::ALL_CONTRACTS);
