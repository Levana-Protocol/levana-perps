use crate::{prelude::*, types::Totals};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet and market
pub(crate) const SHARES: Map<&Addr, NonZero<LpToken>> = Map::new("shares");

/// Total collateral information per market
pub(crate) const TOTALS: Item<Totals> = Item::new("totals");

/// Local cache of markets information
pub(crate) const MARKETS: Item<MarketInfo> = Item::new("markets");

/// Which market is waiting for a reply
pub(crate) const REPLY_MARKET: Item<MarketId> = Item::new("reply-market");
