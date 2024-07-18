use crate::{prelude::*, types::Totals};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet and market
pub(crate) const SHARES: Map<(&Addr, &MarketId), NonZero<LpToken>> = Map::new("shares");

/// Total collateral information per market
pub(crate) const TOTALS: Map<&MarketId, Totals> = Map::new("totals");

/// Local cache of markets information
pub(crate) const MARKETS: Map<&MarketId, MarketInfo> = Map::new("markets");

/// Pending reply state
pub(crate) const REPLY: Item<ReplyState> = Item::new("reply");
