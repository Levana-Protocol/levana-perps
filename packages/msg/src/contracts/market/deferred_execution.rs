//! Deferred execution work items.
//!
//! This allows the protocol to ensure only fresh prices are used for price-sensitive operations.
use std::{fmt, num::ParseIntError};

use crate::constants::event_key;
use cosmwasm_std::StdResult;
use cw_storage_plus::{IntKey, Key, KeyDeserialize, Prefixer, PrimaryKey};
use shared::prelude::*;

use super::{
    entry::{SlippageAssert, StopLoss},
    order::OrderId,
    position::PositionId,
};

/// A unique numeric ID for each deferred execution in the protocol.
#[cw_serde]
#[derive(Copy, PartialOrd, Ord, Eq)]
pub struct DeferredExecId(Uint64);

impl std::hash::Hash for DeferredExecId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.u64().hash(state);
    }
}

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

    const KEY_ELEMS: u16 = 1;

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

/// Result of trying to query a single deferred execution item.
#[cw_serde]
pub enum GetDeferredExecResp {
    /// The requested ID was found
    Found {
        /// The current state of the item
        item: Box<DeferredExecWithStatus>,
    },
    /// The requested ID was not found
    NotFound {},
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
        /// Entity in the system that was impacted by this execution
        target: DeferredExecCompleteTarget,
        /// Timestamp when it was successfully executed
        executed: Timestamp,
    },
    /// Did not successfully apply
    Failure {
        /// Reason it didn't apply successfully
        reason: String,
        /// Timestamp when it failed execution
        executed: Timestamp,
        /// Price point when it was cranked, if applicable
        crank_price: Option<PricePoint>,
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
    /// Is this item a failure?
    pub fn is_failure(&self) -> bool {
        match self {
            DeferredExecStatus::Pending => false,
            DeferredExecStatus::Success { .. } => false,
            DeferredExecStatus::Failure { .. } => true,
        }
    }

    /// Is this item a success?
    pub fn is_success(&self) -> bool {
        match self {
            DeferredExecStatus::Pending => false,
            DeferredExecStatus::Success { .. } => true,
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
        #[deprecated(note = "use take_profit instead")]
        max_gains: Option<MaxGainsInQuote>,
        /// Stop loss price of new position
        stop_loss_override: Option<PriceBaseInQuote>,
        /// Take profit price of new position
        #[serde(alias = "take_profit_override")]
        take_profit: Option<TakeProfitTrader>,
        /// The amount of collateral provided
        amount: NonZero<Collateral>,
        /// Crank fee already charged
        ///
        /// Note that this field only exists for variants where there isn't a
        /// position or order to charge the fee against. In those cases, the position/order
        /// itself is immediately updated to reflect the new charge.
        crank_fee: Collateral,
        /// Crank fee charged, in USD
        crank_fee_usd: Usd,
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

    /// Modify the take profit price of a position
    UpdatePositionTakeProfitPrice {
        /// ID of position to update
        id: PositionId,
        /// New take profit price of the position
        price: TakeProfitTrader,
    },

    /// Modify the stop loss price of a position
    UpdatePositionStopLossPrice {
        /// ID of position to update
        id: PositionId,
        /// New stop loss price of the position
        stop_loss: StopLoss,
    },

    /// Close a position
    ClosePosition {
        /// ID of position to close
        id: PositionId,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
    },

    /// Set a stop loss or take profit override.
    SetTriggerOrder {
        /// ID of position to modify
        id: PositionId,
        /// New stop loss price of the position
        /// Passing None will remove the override.
        stop_loss_override: Option<PriceBaseInQuote>,
        /// New take_profit price of the position
        /// Passing None will bypass changing this
        #[serde(alias = "take_profit_override")]
        take_profit: Option<TakeProfitTrader>,
    },

    /// Set a limit order to open a position when the price of the asset hits
    /// the specified trigger price.
    PlaceLimitOrder {
        /// Price when the order should trigger
        trigger_price: PriceBaseInQuote,
        /// Leverage of new position
        leverage: LeverageToBase,
        /// Direction of new position
        direction: DirectionToBase,
        /// Maximum gains of new position
        #[deprecated(note = "use take_profit instead")]
        max_gains: Option<MaxGainsInQuote>,
        /// Stop loss price of new position
        stop_loss_override: Option<PriceBaseInQuote>,
        /// Take profit price of new position
        #[serde(alias = "take_profit_override")]
        take_profit: Option<TakeProfitTrader>,
        /// The amount of collateral provided
        amount: NonZero<Collateral>,
        /// Crank fee already charged
        crank_fee: Collateral,
        /// Crank fee charged, in USD
        crank_fee_usd: Usd,
    },

    /// Cancel an open limit order
    CancelLimitOrder {
        /// ID of the order
        order_id: OrderId,
    },
}

/// What entity within the system will be affected by this.
#[cw_serde]
#[derive(Copy)]
pub enum DeferredExecTarget {
    /// For open positions or limit orders, no ID exists yet
    DoesNotExist,
    /// Modifying an existing position
    Position(PositionId),
    /// Modifying an existing limit order
    Order(OrderId),
}

/// After successful execution of an item, what did it impact?
///
/// Unlike [DeferredExecTarget] because, after execution, we always have a specific position or order impacted.
#[cw_serde]
#[derive(Copy)]
pub enum DeferredExecCompleteTarget {
    /// Create or Modify an existing position
    Position(PositionId),
    /// Create or Modify an existing limit order
    Order(OrderId),
}

impl DeferredExecTarget {
    /// The position ID, if present
    pub fn position_id(&self) -> Option<PositionId> {
        match self {
            DeferredExecTarget::DoesNotExist | DeferredExecTarget::Order(_) => None,
            DeferredExecTarget::Position(pos_id) => Some(*pos_id),
        }
    }

    /// The order ID, if present
    pub fn order_id(&self) -> Option<OrderId> {
        match self {
            DeferredExecTarget::DoesNotExist | DeferredExecTarget::Position(_) => None,
            DeferredExecTarget::Order(order_id) => Some(*order_id),
        }
    }
}

impl DeferredExecItem {
    /// What entity in the system is targetted by this item.
    pub fn target(&self) -> DeferredExecTarget {
        match self {
            DeferredExecItem::OpenPosition { .. } => DeferredExecTarget::DoesNotExist,
            DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { id, .. } => {
                DeferredExecTarget::Position(*id)
            }
            DeferredExecItem::UpdatePositionAddCollateralImpactSize { id, .. } => {
                DeferredExecTarget::Position(*id)
            }
            DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { id, .. } => {
                DeferredExecTarget::Position(*id)
            }
            DeferredExecItem::UpdatePositionRemoveCollateralImpactSize { id, .. } => {
                DeferredExecTarget::Position(*id)
            }
            DeferredExecItem::UpdatePositionLeverage { id, .. } => {
                DeferredExecTarget::Position(*id)
            }
            DeferredExecItem::UpdatePositionMaxGains { id, .. } => {
                DeferredExecTarget::Position(*id)
            }
            DeferredExecItem::UpdatePositionTakeProfitPrice { id, .. } => {
                DeferredExecTarget::Position(*id)
            }
            DeferredExecItem::UpdatePositionStopLossPrice { id, .. } => {
                DeferredExecTarget::Position(*id)
            }
            DeferredExecItem::ClosePosition { id, .. } => DeferredExecTarget::Position(*id),
            DeferredExecItem::SetTriggerOrder { id, .. } => DeferredExecTarget::Position(*id),
            DeferredExecItem::PlaceLimitOrder { .. } => DeferredExecTarget::DoesNotExist,
            DeferredExecItem::CancelLimitOrder { order_id } => DeferredExecTarget::Order(*order_id),
        }
    }

    /// How much collateral was deposited with this item.
    pub fn deposited_amount(&self) -> Collateral {
        match self {
            DeferredExecItem::OpenPosition { amount, .. }
            | DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { amount, .. }
            | DeferredExecItem::UpdatePositionAddCollateralImpactSize { amount, .. }
            | DeferredExecItem::PlaceLimitOrder { amount, .. } => amount.raw(),
            DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { .. }
            | DeferredExecItem::UpdatePositionRemoveCollateralImpactSize { .. }
            | DeferredExecItem::UpdatePositionLeverage { .. }
            | DeferredExecItem::UpdatePositionMaxGains { .. }
            | DeferredExecItem::UpdatePositionTakeProfitPrice { .. }
            | DeferredExecItem::UpdatePositionStopLossPrice { .. }
            | DeferredExecItem::ClosePosition { .. }
            | DeferredExecItem::SetTriggerOrder { .. }
            | DeferredExecItem::CancelLimitOrder { .. } => Collateral::zero(),
        }
    }
}

/// Event emitted when a deferred execution is queued.
#[derive(Clone, Debug)]
pub struct DeferredExecQueuedEvent {
    /// ID
    pub deferred_exec_id: DeferredExecId,
    /// What entity is targetted by this item
    pub target: DeferredExecTarget,
    /// Address that queued the event
    pub owner: Addr,
}

impl From<DeferredExecQueuedEvent> for Event {
    fn from(
        DeferredExecQueuedEvent {
            deferred_exec_id,
            target,
            owner,
        }: DeferredExecQueuedEvent,
    ) -> Self {
        let mut event = Event::new("deferred-exec-queued")
            .add_attribute(event_key::DEFERRED_EXEC_ID, deferred_exec_id.to_string())
            .add_attribute(event_key::DEFERRED_EXEC_OWNER, owner);
        match target {
            DeferredExecTarget::DoesNotExist => {
                event = event.add_attribute(event_key::DEFERRED_EXEC_TARGET, "does-not-exist");
            }
            DeferredExecTarget::Position(position_id) => {
                event = event
                    .add_attribute(event_key::POS_ID, position_id.to_string())
                    .add_attribute(event_key::DEFERRED_EXEC_TARGET, "position");
            }
            DeferredExecTarget::Order(order_id) => {
                event = event
                    .add_attribute(event_key::ORDER_ID, order_id.to_string())
                    .add_attribute(event_key::DEFERRED_EXEC_TARGET, "order");
            }
        }
        event
    }
}

impl TryFrom<Event> for DeferredExecQueuedEvent {
    type Error = anyhow::Error;

    fn try_from(evt: Event) -> anyhow::Result<Self> {
        Ok(Self {
            deferred_exec_id: evt
                .u64_attr(event_key::DEFERRED_EXEC_ID)
                .map(DeferredExecId::from_u64)?,
            owner: evt.unchecked_addr_attr(event_key::DEFERRED_EXEC_OWNER)?,
            target: match evt.string_attr(event_key::DEFERRED_EXEC_TARGET)?.as_str() {
                "does-not-exist" => DeferredExecTarget::DoesNotExist,
                "position" => DeferredExecTarget::Position(
                    evt.u64_attr(event_key::POS_ID).map(PositionId::new)?,
                ),
                "order" => {
                    DeferredExecTarget::Order(evt.u64_attr(event_key::ORDER_ID).map(OrderId::new)?)
                }
                _ => anyhow::bail!("invalid deferred exec target"),
            },
        })
    }
}

/// Event when a deferred execution item is executed via the crank.
#[derive(Debug)]
pub struct DeferredExecExecutedEvent {
    /// ID
    pub deferred_exec_id: DeferredExecId,
    /// Entity targeted by this action
    pub target: DeferredExecTarget,
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
            target,
            owner,
            success,
            desc,
        }: DeferredExecExecutedEvent,
    ) -> Self {
        let mut event = Event::new("deferred-exec-executed")
            .add_attribute(event_key::DEFERRED_EXEC_ID, deferred_exec_id.to_string())
            .add_attribute(event_key::DEFERRED_EXEC_OWNER, owner)
            .add_attribute(event_key::SUCCESS, if success { "true" } else { "false" })
            .add_attribute(event_key::DESC, desc);

        match target {
            DeferredExecTarget::DoesNotExist => {
                event = event.add_attribute(event_key::DEFERRED_EXEC_TARGET, "does-not-exist");
            }
            DeferredExecTarget::Position(position_id) => {
                event = event
                    .add_attribute(event_key::POS_ID, position_id.to_string())
                    .add_attribute(event_key::DEFERRED_EXEC_TARGET, "position");
            }
            DeferredExecTarget::Order(order_id) => {
                event = event
                    .add_attribute(event_key::ORDER_ID, order_id.to_string())
                    .add_attribute(event_key::DEFERRED_EXEC_TARGET, "order");
            }
        }
        event
    }
}

impl TryFrom<Event> for DeferredExecExecutedEvent {
    type Error = anyhow::Error;

    fn try_from(evt: Event) -> anyhow::Result<Self> {
        Ok(Self {
            deferred_exec_id: evt
                .u64_attr(event_key::DEFERRED_EXEC_ID)
                .map(DeferredExecId::from_u64)?,
            owner: evt.unchecked_addr_attr(event_key::DEFERRED_EXEC_OWNER)?,
            success: evt.bool_attr(event_key::SUCCESS)?,
            desc: evt.string_attr(event_key::DESC)?,
            target: match evt.string_attr(event_key::DEFERRED_EXEC_TARGET)?.as_str() {
                "does-not-exist" => DeferredExecTarget::DoesNotExist,
                "position" => DeferredExecTarget::Position(
                    evt.u64_attr(event_key::POS_ID).map(PositionId::new)?,
                ),
                "order" => {
                    DeferredExecTarget::Order(evt.u64_attr(event_key::ORDER_ID).map(OrderId::new)?)
                }
                _ => anyhow::bail!("invalid deferred exec target"),
            },
        })
    }
}

/// Event when fees are returned to a user
pub struct FeesReturnedEvent {
    /// Who overpaid the fees and received them back
    pub recipient: Addr,
    /// Amount received in collateral
    pub amount: NonZero<Collateral>,
    /// Current USD amount
    pub amount_usd: NonZero<Usd>,
}

impl From<FeesReturnedEvent> for Event {
    fn from(
        FeesReturnedEvent {
            recipient,
            amount,
            amount_usd,
        }: FeesReturnedEvent,
    ) -> Self {
        Event::new("fees-returned")
            .add_attribute("recipient", recipient.into_string())
            .add_attribute("amount", amount.to_string())
            .add_attribute("amount_usd", amount_usd.to_string())
    }
}
