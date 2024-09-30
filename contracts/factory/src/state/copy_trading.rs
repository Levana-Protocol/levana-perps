use cw_storage_plus::{Item, Map};
use msg::contracts::factory::entry::CopyTradingInfo;
use shared::namespace;

/// Code ID of the copy trading contract
pub(crate) const COPY_TRADING_CODE_ID: Item<u64> = Item::new(namespace::COPY_TRADING_CODE_ID);

/// Contains the mapping of wallet and the copy trading contract address
pub(crate) const COPY_TRADING_ADDRS: Map<&CopyTradingInfo, ()> =
    Map::new(namespace::COPY_TRADING_ADDRS);
