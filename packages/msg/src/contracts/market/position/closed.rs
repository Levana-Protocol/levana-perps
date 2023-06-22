//! Represent closed positions.

use shared::prelude::*;
use std::fmt;
use std::fmt::{Display, Formatter};

use super::{Position, PositionId, PositionQueryResponse};

/// Information on a closed position
#[cw_serde]
pub struct ClosedPosition {
    /// Owner at the time the position closed
    pub owner: Addr,
    /// ID of the position
    pub id: PositionId,
    /// Direction (to base) of the position
    pub direction_to_base: DirectionToBase,
    /// Timestamp the position was created
    pub created_at: Timestamp,
    /// Timestamp of the last liquifunding
    pub liquifunded_at: Timestamp,

    /// The one-time fee paid when opening or updating a position
    ///
    /// this value is the current balance, including all updates
    pub trading_fee_collateral: Collateral,
    /// Cumulative trading fees expressed in USD
    pub trading_fee_usd: Usd,
    /// The ongoing fee paid (and earned!) between positions
    /// to incentivize keeping longs and shorts in balance
    /// which in turn reduces risk for LPs
    ///
    /// This value is the current balance, not a historical record of each payment
    pub funding_fee_collateral: Signed<Collateral>,
    /// Cumulative funding fee in USD
    pub funding_fee_usd: Signed<Usd>,
    /// The ongoing fee paid to LPs to lock up their deposit
    /// as counter-size collateral in this position
    ///
    /// This value is the current balance, not a historical record of each payment
    pub borrow_fee_collateral: Collateral,
    /// Cumulative borrow fee in USD
    pub borrow_fee_usd: Usd,

    /// Cumulative amount of crank fees paid by the position
    pub crank_fee_collateral: Collateral,
    /// Cumulative crank fees in USD
    pub crank_fee_usd: Usd,

    /// Cumulative amount of delta neutrality fees paid by (or received by) the position.
    ///
    /// Positive == outgoing, negative == incoming, like funding_fee.
    pub delta_neutrality_fee_collateral: Signed<Collateral>,
    /// Cumulative delta neutrality fee in USD
    pub delta_neutrality_fee_usd: Signed<Usd>,

    /// Deposit collateral for the position.
    ///
    /// This includes any updates from collateral being added or removed.
    pub deposit_collateral: Signed<Collateral>,

    /// Deposit collateral in USD, using cost basis analysis.
    #[serde(default)]
    pub deposit_collateral_usd: Signed<Usd>,

    /// Final active collateral, the amount sent back to the trader on close
    pub active_collateral: Collateral,

    /// Profit or loss of the position in terms of collateral.
    ///
    /// This is the final collateral send to the trader minus all deposits (including updates).
    pub pnl_collateral: Signed<Collateral>,

    /// Profit or loss, in USD
    ///
    /// This is not simply the PnL in collateral converted to USD. It converts
    /// each individual event to a USD representation using the historical
    /// timestamp. This can be viewed as a _cost basis_ view of PnL.
    pub pnl_usd: Signed<Usd>,

    /// The notional size of the position at close.
    pub notional_size: Signed<Notional>,

    /// Entry price
    pub entry_price_base: PriceBaseInQuote,

    /// the time at which the position is actually closed
    /// if by user: time they sent the message
    /// if by liquidation: liquifunding time
    pub close_time: Timestamp,
    /// needed for calculating final settlement amounts
    /// if by user: same as close time
    /// if by liquidation: first time position became liquidatable
    pub settlement_time: Timestamp,

    /// the reason the position is closed
    pub reason: PositionCloseReason,
}

/// Reason the position was closed
#[cw_serde]
#[derive(Eq, Copy)]
pub enum PositionCloseReason {
    /// Some kind of automated price trigger
    Liquidated(LiquidationReason),
    /// The trader directly chose to close the position
    Direct,
}

/// Reason why a position was liquidated
#[cw_serde]
#[derive(Eq, Copy)]
pub enum LiquidationReason {
    /// True liquidation: insufficient funds in active collateral.
    Liquidated,
    /// Maximum gains were achieved.
    MaxGains,
    /// Stop loss price override was triggered.
    StopLoss,
    /// Specifically take profit override, not max gains.
    TakeProfit,
}

impl Display for LiquidationReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let str = match self {
            LiquidationReason::Liquidated => "liquidated",
            LiquidationReason::MaxGains => "max-gains",
            LiquidationReason::StopLoss => "stop-loss",
            LiquidationReason::TakeProfit => "take-profit",
        };

        write!(f, "{}", str)
    }
}

/// Instructions to close a position.
///
/// Closing a position can occur for multiple reasons: explicit action by the
/// trader, settling price exposure (meaning: you hit a liquidation or take
/// profit), insufficient margin... the point of this data structure is to
/// capture all the information needed by the close position actions to do final
/// settlement on a position and move it to the closed position data structures.
#[derive(Debug)]
pub struct ClosePositionInstructions {
    /// The position in its current state
    pub pos: Position,
    /// Any additional fund transfers that need to be reflected.
    ///
    /// Positive value means a transfer from counter collateral to active
    /// collateral. Negative means active to counter collateral. This is not
    /// reflected in the position itself, since Position requires non-zero
    /// active and counter collateral, and it's entirely possible we will
    /// consume the entirety of one of those fields.
    pub exposure: Signed<Collateral>,

    /// See [ClosedPosition::close_time]
    pub close_time: Timestamp,
    /// See [ClosedPosition::settlement_time]
    pub settlement_time: Timestamp,
    /// See [ClosedPosition::reason]
    pub reason: PositionCloseReason,
}

/// Outcome of operations which might require closing a position.
///
/// This can apply to liquifunding, settling price exposure, etc.
#[must_use]
#[derive(Debug)]
pub enum MaybeClosedPosition {
    /// The position stayed open, here's the current status
    Open(Position),
    /// We need to close the position
    Close(ClosePositionInstructions),
}

/// Query response intermediate value on a position.
///
/// Positions which are open but need to be liquidated cannot be represented in
/// a [PositionQueryResponse], since many of the calculated fields will be
/// invalid. We use this data type to represent query responses for open
/// positions.
pub enum PositionOrPendingClose {
    /// Position which should remain open.
    Open(Box<PositionQueryResponse>),
    /// The value stored here may change after actual close occurs due to pending payments.
    PendingClose(Box<ClosedPosition>),
}
