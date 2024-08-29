use crate::{prelude::*};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet and market
pub(crate) const SHARES: Map<(&Addr, &MarketId), NonZero<LpToken>> = Map::new("shares");

/// Reverse of SHARES
pub(crate) const REVERSE_SHARES: Map<(&MarketId, &Addr), ()> = Map::new("reverse-shares");

/// Total collateral information per market
pub(crate) const TOTALS: Map<&MarketId, Totals> = Map::new("totals");

/// Local cache of markets information
pub(crate) const MARKETS: Map<&MarketId, MarketInfo> = Map::new("markets");

/// Which market is waiting for a reply
pub(crate) const REPLY_MARKET: Item<MarketId> = Item::new("reply-market");
