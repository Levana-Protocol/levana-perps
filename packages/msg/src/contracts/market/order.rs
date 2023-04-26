//! Data types for limit orders
use cosmwasm_std::{Addr, StdResult};
use cw_storage_plus::{IntKey, Key, KeyDeserialize, Prefixer, PrimaryKey};
use shared::prelude::*;
use std::fmt;
use std::num::ParseIntError;

/// A limit order
#[cw_serde]
pub struct LimitOrder {
    /// ID of the order
    pub order_id: OrderId,
    /// Owner of the order
    pub owner: Addr,
    /// Price where the order will trigger
    pub trigger_price: PriceBaseInQuote,
    /// Deposit collateral
    pub collateral: NonZero<Collateral>,
    /// Leverage
    pub leverage: LeverageToBase,
    /// Direction of the position
    pub direction: DirectionToNotional,
    /// Maximum gains
    pub max_gains: MaxGainsInQuote,
    /// Stop loss price
    pub stop_loss_override: Option<PriceBaseInQuote>,
    /// Take profit price
    pub take_profit_override: Option<PriceBaseInQuote>,
}

/// A unique numeric ID for each order in the protocol.
#[cw_serde]
#[derive(Copy, PartialOrd, Ord, Eq, Hash)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct OrderId(pub u64);

impl OrderId {
    /// Get the underlying `u64` representation of the order ID.
    pub fn u64(self) -> u64 {
        self.0
    }
}

impl<'a> PrimaryKey<'a> for OrderId {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.to_cw_bytes())]
    }
}

impl<'a> Prefixer<'a> for OrderId {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.to_cw_bytes())]
    }
}

impl KeyDeserialize for OrderId {
    type Output = OrderId;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        u64::from_vec(value).map(OrderId)
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for OrderId {
    type Err = ParseIntError;
    fn from_str(src: &str) -> Result<Self, ParseIntError> {
        u64::from_str(src).map(OrderId)
    }
}

/// Events
pub mod events {
    use crate::constants::{event_key, event_val};
    use crate::contracts::market::order::OrderId;
    use crate::contracts::market::position::PositionId;
    use shared::prelude::MarketType::{CollateralIsBase, CollateralIsQuote};
    use shared::prelude::*;

    /// Event when a limit order is placed
    pub struct PlaceLimitOrderEvent {
        /// Unique order ID
        pub order_id: OrderId,
        /// Owner of the order
        pub owner: Addr,
        /// Trigger price
        pub trigger_price: PriceBaseInQuote,
        /// Market type of the contract
        pub market_type: MarketType,
        /// Deposit collateral
        pub collateral: NonZero<Collateral>,
        /// Deposit collateral in USD at current exchange rate
        pub collateral_usd: NonZero<Usd>,
        /// Signed leverage to base (negative == short, positive == long)
        pub leverage: SignedLeverageToBase,
        /// Direction of the position
        pub direction: DirectionToBase,
        /// Maximum gains
        pub max_gains: MaxGainsInQuote,
        /// Stop loss price
        pub stop_loss_override: Option<PriceBaseInQuote>,
        /// Take profit price
        pub take_profit_override: Option<PriceBaseInQuote>,
    }

    impl PerpEvent for PlaceLimitOrderEvent {}
    impl From<PlaceLimitOrderEvent> for Event {
        fn from(src: PlaceLimitOrderEvent) -> Self {
            let mut event = Event::new(event_key::PLACE_LIMIT_ORDER)
                .add_attribute(
                    event_key::MARKET_TYPE,
                    match src.market_type {
                        CollateralIsQuote => event_val::NOTIONAL_BASE,
                        CollateralIsBase => event_val::COLLATERAL_BASE,
                    },
                )
                .add_attribute(event_key::ORDER_ID, src.order_id.to_string())
                .add_attribute(event_key::POS_OWNER, src.owner.to_string())
                .add_attribute(event_key::TRIGGER_PRICE, src.trigger_price.to_string())
                .add_attribute(
                    event_key::MARKET_TYPE,
                    match src.market_type {
                        CollateralIsQuote => event_val::NOTIONAL_BASE,
                        CollateralIsBase => event_val::COLLATERAL_BASE,
                    },
                )
                .add_attribute(event_key::DEPOSIT_COLLATERAL, src.collateral.to_string())
                .add_attribute(
                    event_key::DEPOSIT_COLLATERAL_USD,
                    src.collateral_usd.to_string(),
                )
                .add_attribute(event_key::LEVERAGE_TO_BASE, src.leverage.to_string())
                .add_attribute(event_key::DIRECTION, src.direction.as_str())
                .add_attribute(event_key::MAX_GAINS, src.max_gains.to_string());

            if let Some(stop_loss_override) = src.stop_loss_override {
                event = event.add_attribute(
                    event_key::STOP_LOSS_OVERRIDE,
                    stop_loss_override.to_string(),
                );
            }

            if let Some(take_profit_override) = src.take_profit_override {
                event = event.add_attribute(
                    event_key::TAKE_PROFIT_OVERRIDE,
                    take_profit_override.to_string(),
                );
            }

            event
        }
    }
    impl TryFrom<Event> for PlaceLimitOrderEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> Result<Self, Self::Error> {
            Ok(Self {
                market_type: evt.map_attr_result(event_key::MARKET_TYPE, |s| match s {
                    event_val::NOTIONAL_BASE => Ok(CollateralIsQuote),
                    event_val::COLLATERAL_BASE => Ok(CollateralIsBase),
                    _ => Err(PerpError::unimplemented().into()),
                })?,
                collateral: evt
                    .string_attr(event_key::DEPOSIT_COLLATERAL)?
                    .as_str()
                    .try_into()?,
                collateral_usd: evt
                    .string_attr(event_key::DEPOSIT_COLLATERAL_USD)?
                    .as_str()
                    .try_into()?,
                leverage: SignedLeverageToBase::from_str(
                    &(evt.string_attr(event_key::LEVERAGE_TO_BASE)?),
                )?,
                direction: evt.direction_attr(event_key::DIRECTION)?,
                max_gains: MaxGainsInQuote::from_str(&(evt.string_attr(event_key::MAX_GAINS)?))?,
                order_id: OrderId(evt.u64_attr(event_key::ORDER_ID)?),
                owner: evt.unchecked_addr_attr(event_key::POS_OWNER)?,
                trigger_price: PriceBaseInQuote::try_from_number(
                    evt.number_attr(event_key::TRIGGER_PRICE)?,
                )?,
                stop_loss_override: match evt.try_number_attr(event_key::STOP_LOSS_OVERRIDE)? {
                    None => None,
                    Some(stop_loss_override) => {
                        Some(PriceBaseInQuote::try_from_number(stop_loss_override)?)
                    }
                },
                take_profit_override: match evt.try_number_attr(event_key::TAKE_PROFIT_OVERRIDE)? {
                    None => None,
                    Some(take_profit_override) => {
                        Some(PriceBaseInQuote::try_from_number(take_profit_override)?)
                    }
                },
            })
        }
    }

    /// A limit order was canceled
    pub struct CancelLimitOrderEvent {
        /// ID of the canceled order
        pub order_id: OrderId,
    }

    impl PerpEvent for CancelLimitOrderEvent {}
    impl From<CancelLimitOrderEvent> for Event {
        fn from(src: CancelLimitOrderEvent) -> Self {
            Event::new(event_key::PLACE_LIMIT_ORDER)
                .add_attribute(event_key::ORDER_ID, src.order_id.to_string())
        }
    }
    impl TryFrom<Event> for CancelLimitOrderEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> Result<Self, Self::Error> {
            Ok(Self {
                order_id: OrderId(evt.u64_attr(event_key::ORDER_ID)?),
            })
        }
    }

    /// A limit order was triggered
    pub struct ExecuteLimitOrderEvent {
        /// ID of the order
        pub order_id: OrderId,
        /// ID of the position, if it successfully opened
        pub pos_id: Option<PositionId>,
    }

    impl PerpEvent for ExecuteLimitOrderEvent {}
    impl From<ExecuteLimitOrderEvent> for Event {
        fn from(src: ExecuteLimitOrderEvent) -> Self {
            let mut event = Event::new(event_key::PLACE_LIMIT_ORDER)
                .add_attribute(event_key::ORDER_ID, src.order_id.to_string());

            if let Some(pos_id) = src.pos_id {
                event = event.add_attribute(event_key::POS_ID, pos_id.to_string());
            }

            event
        }
    }
    impl TryFrom<Event> for ExecuteLimitOrderEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> Result<Self, Self::Error> {
            Ok(Self {
                order_id: OrderId(evt.u64_attr(event_key::ORDER_ID)?),
                pos_id: evt.try_u64_attr(event_key::POS_ID)?.map(PositionId),
            })
        }
    }
}
