use msg::contracts::market::{deferred_execution::DeferredExecId, position::PositionId};

use crate::{
    prelude::*,
    types::{
        EarmarkedItem, LpTokenStatus, LpTokenValue, MarketInfo, MarketWorkInfo, PauseStatus, PositionInfo, QueuePosition, Totals, WalletFund
    },
};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet
pub(crate) const SHARES: Map<(&Token, &Addr), NonZero<LpToken>> = Map::new("shares");

/// The current pause status
pub(crate) const PAUSE_STATUS: Item<PauseStatus> = Item::new("pause-status");

/// Pending queue items that needs to be processed
pub(crate) const PENDING_QUEUE_ITEMS: Map<&QueuePositionId, QueuePosition> =
    Map::new("pending-queue-items");

/// Pending earmarked items that needs to be processed for withdrawal
pub(crate) const PENDING_EARMARKED_ITEMS: Map<&EarmarkId, EarmarkedItem> =
    Map::new("pending-earmarked-items");

/// Last processed queue id
pub(crate) const LAST_PROCESSED_QUEUE_ID: Item<Option<&QueuePositionId>> =
    Item::new("last-processed-queue-id");

/// Last processed earmark id
pub(crate) const LAST_PROCESSED_EARMARK_ID: Item<Option<&EarmarkId>> =
    Item::new("last-processed-earmark-id");

/// Total collateral information
pub(crate) const TOTALS: Map<&Token, Totals> = Map::new("totals");

/// LpToken Value
pub(crate) const LP_TOKEN_VALUE: Item<LpTokenValue> = Item::new("lp-token-value");

/// Work item information per market
pub(crate) const MARKET_WORK_INFO: Map<&MarketId, MarketWorkInfo> = Map::new("market-work-info");

/// Local cache of markets information
pub(crate) const MARKETS: Map<&MarketId, MarketInfo> = Map::new("markets");

// todo:Probably deposit leaders shares separately since it's always going to be accessed whenever someone deposit
