use shared::time::Timestamp;

use crate::{
    prelude::*,
    types::{
        DecQueuePosition, IncQueuePosition, LpTokenValue, MarketInfo, MarketLoaderStatus,
        MarketWorkInfo, Totals, WalletInfo,
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

/// LpToken Value
pub(crate) const LP_TOKEN_VALUE: Map<&Token, LpTokenValue> = Map::new("lp-token-value");

/// Work item information per market
pub(crate) const MARKET_WORK_INFO: Map<&MarketId, MarketWorkInfo> = Map::new("market-work-info");

/// Local cache of markets information
pub(crate) const MARKETS: Map<&MarketId, MarketInfo> = Map::new("markets");

/// Local cache of markets information
pub(crate) const MARKETS_TOKEN: Map<(Token, MarketId), MarketInfo> = Map::new("markets-token");

/// When did the factory was queried last time to check if new market was added ?
pub(crate) const LAST_MARKET_ADD_CHECK: Item<Timestamp> = Item::new("last-market-add-check");

/// Status of the market loader
pub(crate) const MARKET_LOADER_STATUS: Item<MarketLoaderStatus> = Item::new("market-loader-status");
