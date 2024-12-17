use cw_storage_plus::{Item, Map};
use perpswap::contracts::factory::entry::CounterTradeAddr;
use perpswap::namespace;
use perpswap::storage::MarketId;

/// Code ID of the counter trade contract id
pub(crate) const COUNTER_TRADE_CODE_ID: Item<u64> = Item::new(namespace::COUNTERTRADE_CODE_ID);

/// Contains the mapping of wallet and the copy trading contract address
pub(crate) const COUNTER_TRADE_ADDRS: Map<(MarketId, CounterTradeAddr), ()> =
    Map::new(namespace::COUNTER_TRADE_ADDRS);
