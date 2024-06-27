//! Represent closed positions.

use shared::prelude::*;
use std::fmt;
use std::fmt::{Display, Formatter};

use super::{LiquidationMargin, Position, PositionId, PositionQueryResponse};

/// Information on a closed position
#[cw_serde]
pub struct ClosedPosition {
    /// Owner at the time the position closed
    pub owner: Addr,
    /// ID of the position
    pub id: PositionId,
    /// Direction (to base) of the position
    pub direction_to_base: DirectionToBase,
    /// Timestamp the position was created, block time.
    pub created_at: Timestamp,
    /// Timestamp of the price point used for creating this position.
    pub price_point_created_at: Option<Timestamp>,
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
    ///
    /// This will always be the block time when the crank closed the position,
    /// whether via liquidation, deferred execution of a ClosePosition call, or
    /// liquifunding.
    pub close_time: Timestamp,
    /// needed for calculating final settlement amounts
    /// if by user: same as close time
    /// if by liquidation: first time position became liquidatable
    pub settlement_time: Timestamp,

    /// the reason the position is closed
    pub reason: PositionCloseReason,

    /// liquidation margin at the time of close
    /// Optional for the sake of backwards-compatibility
    pub liquidation_margin: Option<LiquidationMargin>,
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

impl Display for PositionCloseReason {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            PositionCloseReason::Liquidated(reason) => write!(f, "{reason}"),
            PositionCloseReason::Direct => f.write_str("Manual close"),
        }
    }
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
#[derive(Clone, Debug)]
pub struct ClosePositionInstructions {
    /// The position in its current state
    pub pos: Position,
    /// The capped exposure amount after taking liquidation margin into account.
    ///
    /// Positive value means a transfer from counter collateral to active
    /// collateral. Negative means active to counter collateral. This is not
    /// reflected in the position itself, since Position requires non-zero
    /// active and counter collateral, and it's entirely possible we will
    /// consume the entirety of one of those fields.
    pub capped_exposure: Signed<Collateral>,
    /// Additional losses that the trader experienced that cut into liquidation margin.
    ///
    /// If the trader
    /// experienced max gains, then this value is 0. In the case where the trader
    /// experienced a liquidation event and capped_exposure did not fully represent
    /// losses due to liquidation margin, this value contains additional losses we would
    /// like to take away from the trader after paying all pending fees.
    pub additional_losses: Collateral,

    /// The price point used for settling this position.
    pub settlement_price: PricePoint,
    /// See [ClosedPosition::reason]
    pub reason: PositionCloseReason,

    /// Did this occur because the position was closed during liquifunding?
    pub closed_during_liquifunding: bool,
}

/// Outcome of operations which might require closing a position.
///
/// This can apply to liquifunding, settling price exposure, etc.
#[must_use]
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum MaybeClosedPosition {
    /// The position stayed open, here's the current status
    Open(Position),
    /// We need to close the position
    Close(ClosePositionInstructions),
}

impl From<MaybeClosedPosition> for Position {
    fn from(maybe_closed: MaybeClosedPosition) -> Self {
        match maybe_closed {
            MaybeClosedPosition::Open(pos) => pos,
            MaybeClosedPosition::Close(instructions) => instructions.pos,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ser_de_position_close_reason() {
        for value in [
            PositionCloseReason::Direct,
            PositionCloseReason::Liquidated(LiquidationReason::Liquidated),
            PositionCloseReason::Liquidated(LiquidationReason::MaxGains),
            PositionCloseReason::Liquidated(LiquidationReason::StopLoss),
            PositionCloseReason::Liquidated(LiquidationReason::TakeProfit),
        ] {
            let json = serde_json::to_string(&value).unwrap();
            let parsed = serde_json::from_str(&json).unwrap();
            assert_eq!(value, parsed);
        }
    }
}
