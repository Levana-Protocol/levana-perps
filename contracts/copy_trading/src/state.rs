use msg::contracts::market::position::PositionId;

use crate::{prelude::*, types::{MarketInfo, MarketTotals, Totals, WalletFund}};

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

/// Lock LpToken for withdrawal when available
pub(crate) const LOCKED_SHARES: Map<&Addr, Vec<WalletFund>> = Map::new("locked-share");

/// Outstanding funds separated for withdrawal
pub(crate) const OUTSTANDING_FUNDS: Item<Totals> = Item::new("outstanding-funds");

///
//pub(crate) const OPEN_DEFERRED_EXEC
