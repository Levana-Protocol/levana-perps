//! Deferred execution work items.
//!
//! This allows the protocol to ensure only fresh prices are used for price-sensitive operations.
use std::{fmt, num::ParseIntError};

use cosmwasm_std::StdResult;
use cw_storage_plus::{IntKey, Key, KeyDeserialize, Prefixer, PrimaryKey};
use shared::prelude::*;

use super::{entry::SlippageAssert, position::PositionId};

/// A unique numeric ID for each deferred execution in the protocol.
#[cw_serde]
#[derive(Copy, PartialOrd, Ord, Eq)]
pub struct DeferredExecId(Uint64);

impl DeferredExecId {
    /// First ID issued. We start with 1 instead of 0 for user friendliness.
    pub fn first() -> Self {
        DeferredExecId(Uint64::one())
    }

    /// Get the next deferred exec ID. Will panic if you overflow.
    pub fn next(self) -> Self {
        DeferredExecId((self.0.u64() + 1).into())
    }

    /// Get the underlying `u64` representation.
    pub fn u64(self) -> u64 {
        self.0.u64()
    }

    /// Generate from a raw u64
    pub fn from_u64(x: u64) -> Self {
        DeferredExecId(x.into())
    }
}

impl<'a> PrimaryKey<'a> for DeferredExecId {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.u64().to_cw_bytes())]
    }
}

impl<'a> Prefixer<'a> for DeferredExecId {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.u64().to_cw_bytes())]
    }
}

impl KeyDeserialize for DeferredExecId {
    type Output = DeferredExecId;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        u64::from_vec(value).map(|x| DeferredExecId(Uint64::new(x)))
    }
}

impl fmt::Display for DeferredExecId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DeferredExecId {
    type Err = ParseIntError;
    fn from_str(src: &str) -> Result<Self, ParseIntError> {
        src.parse().map(|x| DeferredExecId(Uint64::new(x)))
    }
}

/// Enumeration API for getting deferred exec IDs
#[cw_serde]
pub struct ListDeferredExecsResp {
    /// Next batch of items
    pub items: Vec<DeferredExecWithStatus>,
    /// Only `Some` if more IDs exist
    pub next_start_after: Option<DeferredExecId>,
}

/// A deferred execution work item and its current status.
#[cw_serde]
pub struct DeferredExecWithStatus {
    /// ID of this item
    pub id: DeferredExecId,
    /// Timestamp this was created, and therefore minimum price update timestamp needed
    pub created: Timestamp,
    /// Status
    pub status: DeferredExecStatus,
    /// Who owns (i.e. created) this item?
    pub owner: Addr,
    /// Work item
    pub item: DeferredExecItem,
}

/// Current status of a deferred execution work item
#[cw_serde]
pub enum DeferredExecStatus {
    /// Waiting to be cranked
    Pending,
    /// Successfully applied
    Success {
        /// Position ID, either created, updated, or closed
        id: PositionId,
    },
    /// Did not successfully apply
    Failure {
        /// Reason it didn't apply successfully
        reason: String,
    },
}

impl DeferredExecStatus {
    /// Is this item still pending execution?
    pub fn is_pending(&self) -> bool {
        match self {
            DeferredExecStatus::Pending => true,
            DeferredExecStatus::Success { .. } => false,
            DeferredExecStatus::Failure { .. } => false,
        }
    }
}

/// A deferred execution work item
#[cw_serde]
#[allow(clippy::large_enum_variant)]
pub enum DeferredExecItem {
    /// Open a new position
    OpenPosition {
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
        /// Leverage of new position
        leverage: LeverageToBase,
        /// Direction of new position
        direction: DirectionToBase,
        /// Maximum gains of new position
        max_gains: MaxGainsInQuote,
        /// Stop loss price of new position
        stop_loss_override: Option<PriceBaseInQuote>,
        /// Take profit price of new position
        take_profit_override: Option<PriceBaseInQuote>,
        /// The amount of collateral provided
        amount: NonZero<Collateral>,
    },
    /// Add collateral to a position, causing leverage to decrease
    ///
    /// The amount of collateral to add must be attached as funds
    UpdatePositionAddCollateralImpactLeverage {
        /// ID of position to update
        id: PositionId,
        /// The amount of collateral provided
        amount: NonZero<Collateral>,
    },
    /// Add collateral to a position, causing notional size to increase
    ///
    /// The amount of collateral to add must be attached as funds
    UpdatePositionAddCollateralImpactSize {
        /// ID of position to update
        id: PositionId,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
        /// The amount of collateral provided
        amount: NonZero<Collateral>,
    },

    /// Remove collateral from a position, causing leverage to increase
    UpdatePositionRemoveCollateralImpactLeverage {
        /// ID of position to update
        id: PositionId,
        /// Amount of funds to remove from the position
        amount: NonZero<Collateral>,
    },
    /// Remove collateral from a position, causing notional size to decrease
    UpdatePositionRemoveCollateralImpactSize {
        /// ID of position to update
        id: PositionId,
        /// Amount of funds to remove from the position
        amount: NonZero<Collateral>,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
    },

    /// Modify the leverage of the position
    ///
    /// This will impact the notional size of the position
    UpdatePositionLeverage {
        /// ID of position to update
        id: PositionId,
        /// New leverage of the position
        leverage: LeverageToBase,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
    },

    /// Modify the max gains of a position
    UpdatePositionMaxGains {
        /// ID of position to update
        id: PositionId,
        /// New max gains of the position
        max_gains: MaxGainsInQuote,
    },

    /// Close a position
    ClosePosition {
        /// ID of position to close
        id: PositionId,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
    },
}

impl DeferredExecItem {
    /// The position ID for this item, if present.
    pub fn position_id(&self) -> Option<PositionId> {
        match self {
            DeferredExecItem::OpenPosition { .. } => None,
            DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { id, .. } => Some(*id),
            DeferredExecItem::UpdatePositionAddCollateralImpactSize { id, .. } => Some(*id),
            DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { id, .. } => Some(*id),
            DeferredExecItem::UpdatePositionRemoveCollateralImpactSize { id, .. } => Some(*id),
            DeferredExecItem::UpdatePositionLeverage { id, .. } => Some(*id),
            DeferredExecItem::UpdatePositionMaxGains { id, .. } => Some(*id),
            DeferredExecItem::ClosePosition { id, .. } => Some(*id),
        }
    }

    /// How much collateral was deposited with this item.
    pub fn deposited_amount(&self) -> Collateral {
        match self {
            DeferredExecItem::OpenPosition { amount, .. }
            | DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { amount, .. }
            | DeferredExecItem::UpdatePositionAddCollateralImpactSize { amount, .. } => {
                amount.raw()
            }
            DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { .. }
            | DeferredExecItem::UpdatePositionRemoveCollateralImpactSize { .. }
            | DeferredExecItem::UpdatePositionLeverage { .. }
            | DeferredExecItem::UpdatePositionMaxGains { .. }
            | DeferredExecItem::ClosePosition { .. } => Collateral::zero(),
        }
    }
}

/// Event emitted when a deferred execution is queued.
pub struct DeferredExecQueuedEvent {
    /// ID
    pub deferred_exec_id: DeferredExecId,
    /// If relevant, position ID impacted by this
    pub position_id: Option<PositionId>,
    /// Address that queued the event
    pub owner: Addr,
}

impl From<DeferredExecQueuedEvent> for Event {
    fn from(
        DeferredExecQueuedEvent {
            deferred_exec_id,
            position_id,
            owner,
        }: DeferredExecQueuedEvent,
    ) -> Self {
        let mut event = Event::new("deferred-exec-queued")
            .add_attribute("deferred_exec_id", deferred_exec_id.to_string())
            .add_attribute("owner", owner);
        if let Some(position_id) = position_id {
            event = event.add_attribute("pos-id", position_id.to_string());
        }
        event
    }
}

/// Event when a deferred execution item is executed via the crank.
pub struct DeferredExecExecutedEvent {
    /// ID
    pub deferred_exec_id: DeferredExecId,
    /// If relevant, position ID impacted by this
    pub position_id: Option<PositionId>,
    /// Address that owns this item
    pub owner: Addr,
    /// Was this item executed successfully?
    pub success: bool,
    /// Text description of what happened
    pub desc: String,
}

impl From<DeferredExecExecutedEvent> for Event {
    fn from(
        DeferredExecExecutedEvent {
            deferred_exec_id,
            position_id,
            owner,
            success,
            desc,
        }: DeferredExecExecutedEvent,
    ) -> Self {
        let mut event = Event::new("deferred-exec-executed")
            .add_attribute("deferred_exec_id", deferred_exec_id.to_string())
            .add_attribute("owner", owner)
            .add_attribute("success", if success { "true" } else { "false" })
            .add_attribute("desc", desc);
        if let Some(position_id) = position_id {
            event = event.add_attribute("pos-id", position_id.to_string());
        }
        event
    }
}
