use msg::contracts::market::{deferred_execution::DeferredExecId, position::PositionId};

use crate::{
    prelude::*,
    types::{MarketInfo, MarketTotals, PositionInfo, Totals, WalletFund},
};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet
pub(crate) const SHARES: Map<&Addr, NonZero<LpToken>> = Map::new("shares");

/// Total pending collateral information. It's pending as the queue is
/// not yet processed. Once it's processed, the collateral will move
/// into [TOTALS].
pub(crate) const PENDING_DEPOSITS: Map<&QueuePositionId, (&Addr, NonZero<Collateral>)> =
    Map::new("pending-deposits");

/// Last processed queue id
pub(crate) const LAST_PROCESSED_QUEUE_ID: Item<Option<&QueuePositionId>> =
    Item::new("last-processed-queue-id");

/// Last processed deferred exec id
pub(crate) const LAST_PROCESSED_DEFERRED_EXEC_ID: Item<Option<&DeferredExecId>> =
    Item::new("last-processed-deferred-exec-id");

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

/// Open position ids for the market
pub(crate) const MARKET_OPEN_POSITION_IDS: Map<(&MarketId, &PositionId), PositionInfo> =
    Map::new("market-open-position-ids");

/// Pending open positions, that needs to be processed
pub(crate) const MARKET_QUEUE_OPEN: Map<(&MarketId, &DeferredExecId), ()> =
    Map::new("market-queue-open");

/// Pending closed positions, that needs to be processed
pub(crate) const MARKET_QUEUE_CLOSE: Map<(&MarketId, &DeferredExecId), ()> =
    Map::new("market-queue-close");

/// Pending updated positions, that needs to be processed
pub(crate) const MARKET_QUEUE_UPDATE: Map<(&MarketId, &DeferredExecId), ()> =
    Map::new("market-queue-update");
