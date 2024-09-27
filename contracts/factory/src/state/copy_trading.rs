use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use shared::namespace;

pub(crate) const COPY_TRADING_CODE_ID: Item<u64> = Item::new(namespace::COPY_TRADING_CODE_ID);

pub(crate) const COPY_TRADING_ADDRS: Map<&Addr, ()> = Map::new(namespace::COPY_TRADING_ADDRS);
