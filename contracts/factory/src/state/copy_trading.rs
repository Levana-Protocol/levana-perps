use cw_storage_plus::{Item, Map};
use perpswap::contracts::factory::entry::{CopyTradingAddr, LeaderAddr};
use perpswap::namespace;

/// Code ID of the copy trading contract
pub(crate) const COPY_TRADING_CODE_ID: Item<u64> = Item::new(namespace::COPY_TRADING_CODE_ID);

/// Contains the mapping of wallet and the copy trading contract address
pub(crate) const COPY_TRADING_ADDRS: Map<(LeaderAddr, CopyTradingAddr), ()> =
    Map::new(namespace::COPY_TRADING_ADDRS);
