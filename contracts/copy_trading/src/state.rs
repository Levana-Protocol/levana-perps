use msg::contracts::market::position::PositionId;

use crate::{prelude::*, types::{MarketInfo, MarketTotals, Totals}};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet
pub(crate) const SHARES: Map<&Addr, NonZero<LpToken>> = Map::new("shares");

/// Total collateral information
pub(crate) const TOTALS: Item<Totals> = Item::new("totals");

/// Total collateral information per market
pub(crate) const MARKET_TOTALS: Map<&MarketId, MarketTotals> = Map::new("market-totals");

/// Local cache of markets information
pub(crate) const MARKETS: Map<&MarketId, MarketInfo> = Map::new("markets");

/// Which market is waiting for a reply
pub(crate) const REPLY_MARKET: Item<MarketId> = Item::new("reply-market");
