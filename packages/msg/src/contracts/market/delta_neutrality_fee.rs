//! Events around the delta neutrality fee
use cosmwasm_std::Event;
use perpswap::prelude::*;

/// Event when a delta neutrality payment is made.
#[derive(Clone)]
pub struct DeltaNeutralityFeeEvent {
    /// Amount of the fee. Negative means paid to trader.
    pub amount: Signed<Collateral>,
    /// Fund size before
    pub total_funds_before: Collateral,
    /// Fund size after
    pub total_funds_after: Collateral,
    /// Action taken by trader
    pub reason: DeltaNeutralityFeeReason,
    /// Amount taken for the protocol tax
    pub protocol_amount: Collateral,
}

impl From<DeltaNeutralityFeeEvent> for Event {
    fn from(src: DeltaNeutralityFeeEvent) -> Self {
        Event::new("delta-neutrality-fee").add_attributes(vec![
            ("amount", src.amount.to_string()),
            ("total-funds-before", src.total_funds_before.to_string()),
            ("total-funds-after", src.total_funds_after.to_string()),
            ("reason", src.reason.as_str().to_string()),
        ])
    }
}

/// Action taken by trader to incur a delta neutrality fee
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeltaNeutralityFeeReason {
    /// Open a new position
    PositionOpen,
    /// Update an existing position
    PositionUpdate,
    /// Close a position
    PositionClose,
}

impl DeltaNeutralityFeeReason {
    /// Express the reason as a string
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::PositionOpen => "open",
            Self::PositionUpdate => "update",
            Self::PositionClose => "close",
        }
    }
}
