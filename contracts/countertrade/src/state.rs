use crate::{prelude::*, types::Totals};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet and market
pub(crate) const SHARES: Map<(&Addr, &MarketId), NonZero<LpToken>> = Map::new("shares");

/// Total collateral information per market
pub(crate) const TOTALS: Map<&MarketId, Totals> = Map::new("totals");
