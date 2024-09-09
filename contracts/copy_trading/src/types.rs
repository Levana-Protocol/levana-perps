use std::fmt::Display;

use msg::contracts::market::{
    deferred_execution::DeferredExecId, entry::ClosedPositionCursor, order::OrderId, position::PositionId
};
use shared::{number::Usd, time::Timestamp};

use crate::prelude::*;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct MarketInfo {
    /// Market id
    pub(crate) id: MarketId,
    /// Market address
    pub(crate) addr: Addr,
    /// Token used by the market
    pub(crate) token: msg::token::Token,
}

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) config: Config,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) my_addr: Addr,
}

/// Total LP share information
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub(crate) struct Totals {
    /// Total collateral still in this contract.
    ///
    /// Collateral used by active positions is excluded.
    pub(crate) collateral: Collateral,
    /// Total LP shares
    pub(crate) shares: LpToken,
}

/// Market information related to the work performed
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct MarketWorkInfo {
    /// The last seen position id. Should be passed to
    /// [msg::contracts::position_token::entry::QueryMsg::Tokens]
    pub(crate) tokens_start_after: Option<String>,
    /// The last closed position cursor. Should be passed to
    /// [msg::contracts::market::entry::QueryMsg::ClosedPositionHistory]
    pub(crate) last_closed_cursor: Option<ClosedPositionCursor>,
    /// The latest deferred exec item we're waiting on. Should be
    /// passed to
    /// [msg::contracts::market::entry::QueryMsg::ListDeferredExecs]
    pub(crate) deferred_exec_start_after: Option<DeferredExecId>,
    /// Last seen limit order. Should be passed to
    /// [msg::contracts::market::entry::QueryMsg::LimitOrders]
    pub(crate) limit_order_start_after: Option<OrderId>,
    /// Last seen limit order. Should be passed to
    /// [msg::contracts::market::entry::QueryMsg::LimitOrderHistory]
    pub(crate) limit_order_history_next_start_after: Option<String>,
    /// Total deposit collateral locked on orders
    pub(crate) total_orders_collateral: Collateral,
    /// Total active collateral seen so far the open positions
    pub(crate) total_active_collateral: Collateral,
    /// Status of the Work information for processing open positions
    pub(crate) open_status: MarketWorkStatus,
    /// Status of the Work information for processing open positions
    pub(crate) close_status: MarketWorkStatus,
    /// Status of the Work information for processing orders
    pub(crate) order_status: MarketWorkStatus,
    /// Status of the Work information for deferred order items
    pub(crate) deferred_exec_status: MarketWorkStatus,
    /// Stats of this Market
    pub(crate) stats: MarketStats
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct MarketStats {
    /// Total profit so far in the closed positions
    profit_in_usd: Usd,
    /// Total loss so far in the closed positions
    loss_in_usd: Usd,
    /// Sum of deposit collateral of all open positions
    tvl_open_positions_usd: Usd,
    /// Sum of deposit collateral of all closed positions
    tvl_closed_positions_usd: Usd
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) enum MarketWorkStatus {
    /// Still pending
    Pending,
    /// Finished
    Finished,
    /// Not started
    NotStarted
}

/// Specific position information
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct PositionInfo {
    /// Unique identifier for a position
    pub(crate) id: PositionId,
    /// Active collateral for the position
    pub(crate) active_collateral: NonZero<Collateral>,
    /// Unrealized PnL on this position, in terms of collateral.
    pub(crate) pnl_collateral: Signed<Collateral>,
    /// Unrealized PnL on this position, in USD, using cost-basis analysis.
    pub(crate) pnl_usd: Signed<Usd>,
}

/// Specific wallet fund
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct WalletFund {
    /// LP Shares that is locked
    pub(crate) share: NonZero<LpToken>,
    /// Equivalent collateral amount for the LpToken
    pub(crate) collateral: NonZero<Collateral>,
    /// Timestamp locked at
    pub(crate) locked_at: Timestamp,
}

/// LpToken Value
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct LpTokenValue {
    /// Value of one LpToken
    pub(crate) value: NonZero<Collateral>,
    /// Status of the value
    pub(crate) status: LpTokenStatus,
    /// Timestamp the value was last computed
    pub(crate) timestamp: Timestamp,
}

/// Status of [LpTokenValue]
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) enum LpTokenStatus {
    /// Recently computed and valid for other computations
    Valid,
    ////Outdated because of open positions etc. Need to be computed
    /// again.
    Outdated,
}

/// Queue position
pub(crate) struct QueuePosition {
    /// Queue item that needs to be processed
    item: QueueItem,
    /// Wallet that initiated the specific item action
    wallet: Addr,
}

/// Queue item that needs to be processed
pub(crate) enum QueueItem {
    /// Deposit the fund and get some [LpToken]
    Deposit { funds: NonZero<Collateral> },
    /// Withdraw via LpToken
    Withdrawal { tokens: NonZero<LpToken> },
    /// Open Position etc. etc.
    OpenPosition {}
}

/// Checks if the pause is status
pub(crate) enum PauseStatus {
    /// Paused because queue items are processed
    PauseQueueProcessed {
        /// Does a stats reset required ?
        reset_required: bool
    },
    /// Paused because of earmarking
    PauseReasonEarmarking {
        /// Does a stats reset required ?
        reset_required: bool
    },
    /// Not paused
    NotPaused,
}

/// Earmarked item
pub(crate) struct EarmarkedItem {
    /// Wallet
    wallet: Addr,
    /// Tokens that have been earmarked
    tokens: NonZero<LpToken>,
    /// Required collateral when last time [LpTokenStatus] was
    /// valid. We try updating the LpTokenStatus only if
    /// require_collateral is less than current available collateral.
    outdated_required_collateral: Collateral
}
