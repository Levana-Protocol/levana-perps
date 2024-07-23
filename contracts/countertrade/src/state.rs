use crate::{prelude::*, types::Totals};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet and market
pub(crate) const SHARES: Map<(&Addr, &MarketId), NonZero<LpToken>> = Map::new("shares");

pub(crate) const SHARES_CURSOR: Item<ResetSharesCursor> = Item::new("reset-shares");

/// Total collateral information per market
pub(crate) const TOTALS: Map<&MarketId, Totals> = Map::new("totals");

/// Local cache of markets information
pub(crate) const MARKETS: Map<&MarketId, MarketInfo> = Map::new("markets");

/// Which market is waiting for a reply
pub(crate) const REPLY_MARKET: Item<MarketId> = Item::new("reply-market");
