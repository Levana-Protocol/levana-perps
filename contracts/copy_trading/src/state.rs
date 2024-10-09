use perpswap::contracts::market::{
    deferred_execution::DeferredExecId, entry::ClosedPositionCursor,
};
use perpswap::time::Timestamp;

use crate::{
    prelude::*,
    types::{
        BatchWork, DecQueuePosition, HighWaterMark, IncQueuePosition, LpTokenValue, MarketInfo,
        MarketLoaderStatus, MarketWorkInfo, Totals, WalletInfo,
    },
};

/// Overall config
pub(crate) const CONFIG: Item<Config> = Item::new("config");

/// Shares held per wallet
pub(crate) const SHARES: Map<&WalletInfo, NonZero<LpToken>> = Map::new("shares");

/// Pending queued items for a wallet
pub(crate) const WALLET_QUEUE_ITEMS: Map<(&Addr, QueuePositionId), ()> =
    Map::new("wallet-queue-items");

/// Pending queue items that needs to be processed. The queue item
/// contains that item that will increase or not change the available collateral
/// like closing a position.
pub(crate) const COLLATERAL_INCREASE_QUEUE: Map<&IncQueuePositionId, IncQueuePosition> =
    Map::new("collateral-increase-queue");

/// Pending queue items that needs to be processed. The queue item
/// contains that item that will decrease the available collateral
/// like opening a position.
pub(crate) const COLLATERAL_DECREASE_QUEUE: Map<&DecQueuePositionId, DecQueuePosition> =
    Map::new("collateral-decrease-queue");

/// Last inserted queue id in [COLLATERAL_INCREASE_QUEUE]
pub(crate) const LAST_INSERTED_INC_QUEUE_ID: Item<IncQueuePositionId> =
    Item::new("last-inserted-inc-queue-id");

/// Last inserted queue id in [COLLATERAL_DECREASE_QUEUE]
pub(crate) const LAST_INSERTED_DEC_QUEUE_ID: Item<DecQueuePositionId> =
    Item::new("last-inserted-dec-queue-id");

/// Last processed queue id
pub(crate) const LAST_PROCESSED_INC_QUEUE_ID: Item<IncQueuePositionId> =
    Item::new("last-processed-inc-queue-id");

/// Last processed queue id
pub(crate) const LAST_PROCESSED_DEC_QUEUE_ID: Item<DecQueuePositionId> =
    Item::new("last-processed-dec-queue-id");

/// Total collateral information
pub(crate) const TOTALS: Map<&Token, Totals> = Map::new("totals");

/// Total collateral information
pub(crate) const PENDING_DEPOSITS: Map<&Token, Collateral> = Map::new("pending-deposits");

/// LpToken Value
pub(crate) const LP_TOKEN_VALUE: Map<&Token, LpTokenValue> = Map::new("lp-token-value");

/// Work item information per market
pub(crate) const MARKET_WORK_INFO: Map<&MarketId, MarketWorkInfo> = Map::new("market-work-info");

/// Local cache of markets information
pub(crate) const MARKETS: Map<&MarketId, MarketInfo> = Map::new("markets");

/// Local cache of markets information
pub(crate) const MARKETS_TOKEN: Map<(Token, MarketId), MarketInfo> = Map::new("markets-token");

/// When did we last query the list of markets from the factory? Needed to efficiently check if a new market was added.
pub(crate) const LAST_MARKET_ADD_CHECK: Item<Timestamp> = Item::new("last-market-add-check");

/// Status of the market loader
pub(crate) const MARKET_LOADER_STATUS: Item<MarketLoaderStatus> = Item::new("market-loader-status");

/// Deferred exec status stored in the reply entrypoint
pub(crate) const REPLY_DEFERRED_EXEC_ID: Item<Option<DeferredExecId>> =
    Item::new("reply-deferred-exec-id");

/// Last closed position
pub(crate) const LAST_CLOSED_POSITION_CURSOR: Map<&MarketId, ClosedPositionCursor> =
    Map::new("last-closed-position-cursor");

/// Leader comissision
pub(crate) const LEADER_COMMISSION: Map<&Token, Collateral> = Map::new("leader-commission");

/// High water mark
pub(crate) const HIGH_WATER_MARK: Map<&Token, HighWaterMark> = Map::new("high-water-market");

/// Tracks current batch work that is in progress
pub(crate) const CURRENT_BATCH_WORK: Item<BatchWork> = Item::new("current-batch-work");
