//! Data types and events for cranking.
use super::position::PositionId;
use crate::contracts::market::order::OrderId;
use crate::contracts::market::position::LiquidationReason;
use shared::prelude::*;

/// What work is currently available for the crank.
#[cw_serde]
pub enum CrankWorkInfo {
    /// Closing all open positions
    CloseAllPositions {
        /// Next position to be closed
        position: PositionId,
    },
    /// Resetting all LP balances to 0 after all liquidity is drained
    ResetLpBalances {},
    /// Liquifund a position
    Liquifunding {
        /// Next position to be liquifunded
        position: PositionId,
    },
    /// Adding liquidation prices to the primary data structures
    UnpendLiquidationPrices {
        /// Which position to process next
        position: PositionId,
    },
    /// Liquidate a position.
    ///
    /// Includes max gains, take profit, and stop loss.
    Liquidation {
        /// Position to liquidate
        position: PositionId,
        /// Reason for the liquidation
        liquidation_reason: LiquidationReason,
        /// Timestamp of price update that triggered the liquidation
        price_point_timestamp: Timestamp,
    },
    /// Limit order can be opened
    LimitOrder {
        /// ID of the order to be opened
        order_id: OrderId,
    },
    /// Finished all processing for a given price update
    Completed {
        /// Timestamp of the price update
        price_point_timestamp: Timestamp,
    },
}

/// Events related to the crank
pub mod events {
    use super::*;
    use cosmwasm_std::Event;

    /// Batch processing of multiple cranks
    pub struct CrankExecBatchEvent {
        /// How many cranks were requested
        pub requested: u64,
        /// How many cranks were actually processed
        pub actual: u64,
    }

    impl PerpEvent for CrankExecBatchEvent {}
    impl From<CrankExecBatchEvent> for Event {
        fn from(CrankExecBatchEvent { requested, actual }: CrankExecBatchEvent) -> Self {
            Event::new("crank-batch-exec")
                .add_attribute("requested", requested.to_string())
                .add_attribute("actual", actual.to_string())
        }
    }
    impl TryFrom<Event> for CrankExecBatchEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(CrankExecBatchEvent {
                requested: evt.u64_attr("requested")?,
                actual: evt.u64_attr("actual")?,
            })
        }
    }

    impl PerpEvent for CrankWorkInfo {}
    impl From<CrankWorkInfo> for Event {
        fn from(src: CrankWorkInfo) -> Self {
            let mut event = Event::new("crank-work").add_attribute(
                "kind",
                match src {
                    CrankWorkInfo::CloseAllPositions { .. } => "close-all-positions",
                    CrankWorkInfo::ResetLpBalances { .. } => "reset-lp-balances",
                    CrankWorkInfo::Completed { .. } => "completed",
                    CrankWorkInfo::Liquidation { .. } => "liquidation",
                    CrankWorkInfo::Liquifunding { .. } => "liquifunding",
                    CrankWorkInfo::UnpendLiquidationPrices { .. } => "unpend-liquidation-prices",
                    CrankWorkInfo::LimitOrder { .. } => "limit-order",
                },
            );

            let (position_id, order_id, price_point_timestamp) = match src {
                CrankWorkInfo::CloseAllPositions { position } => (Some(position), None, None),
                CrankWorkInfo::ResetLpBalances {} => (None, None, None),
                CrankWorkInfo::Completed {
                    price_point_timestamp,
                } => (None, None, Some(price_point_timestamp)),
                CrankWorkInfo::Liquidation {
                    position,
                    liquidation_reason: _,
                    price_point_timestamp,
                } => (Some(position), None, Some(price_point_timestamp)),
                CrankWorkInfo::Liquifunding { position } => (Some(position), None, None),
                CrankWorkInfo::UnpendLiquidationPrices { position } => (Some(position), None, None),
                CrankWorkInfo::LimitOrder { order_id } => (None, Some(order_id), None),
            };

            if let Some(position_id) = position_id {
                event = event.add_attribute("pos-id", position_id.to_string());
            }
            if let Some(price_point_timestamp) = price_point_timestamp {
                event =
                    event.add_attribute("price-point-timestamp", price_point_timestamp.to_string());
            }

            if let Some(order_id) = order_id {
                event = event.add_attribute("order-id", order_id.to_string());
            }

            event
        }
    }

    impl TryFrom<Event> for CrankWorkInfo {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            let get_position_id =
                || -> anyhow::Result<PositionId> { Ok(PositionId::new(evt.u64_attr("pos-id")?)) };

            let get_price_point_timestamp = || evt.timestamp_attr("price-point-timestamp");

            let get_liquidation_reason = || -> anyhow::Result<LiquidationReason> {
                match evt.string_attr("liquidation-reason")?.as_str() {
                    "liquidated" => Ok(LiquidationReason::Liquidated),
                    "take-profit" => Ok(LiquidationReason::MaxGains),
                    _ => Err(PerpError::unimplemented().into()),
                }
            };

            evt.map_attr_result("kind", |s| match s {
                "completed" => Ok(CrankWorkInfo::Completed {
                    price_point_timestamp: get_price_point_timestamp()?,
                }),
                "liquifunding" => Ok(CrankWorkInfo::Liquifunding {
                    position: get_position_id()?,
                }),
                "liquidation" => Ok(CrankWorkInfo::Liquidation {
                    position: get_position_id()?,
                    liquidation_reason: get_liquidation_reason()?,
                    price_point_timestamp: get_price_point_timestamp()?,
                }),
                "unpend-liquidation-prices" => Ok(CrankWorkInfo::UnpendLiquidationPrices {
                    position: get_position_id()?,
                }),
                "limit-order" => Ok(CrankWorkInfo::LimitOrder {
                    order_id: OrderId::new(evt.u64_attr("order-id")?),
                }),
                _ => Err(PerpError::unimplemented().into()),
            })
        }
    }
}
