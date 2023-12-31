//! Data types and events for cranking.
use super::deferred_execution::{DeferredExecId, DeferredExecTarget};
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
        /// Price point used for closing all positions
        price_point: PricePoint,
    },
    /// Resetting all LP balances to 0 after all liquidity is drained
    ResetLpBalances {},
    /// Liquifund a position
    Liquifunding {
        /// Next position to be liquifunded
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
        /// price point that triggered the liquidation
        price_point: PricePoint,
    },
    /// Deferred execution (open/update/closed) can be executed.
    DeferredExec {
        /// ID to be processed
        deferred_exec_id: DeferredExecId,
        /// Target of the action
        target: DeferredExecTarget,
        /// Timestamp of the price point that allows execution
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

impl CrankWorkInfo {
    /// Should a cranker receive rewards for performing this action?
    ///
    /// We generally want to give out rewards for actions that are directly
    /// user initiated and will be receiving a crank fee paid into the system. Actions
    /// which are overall protocol maintenance without a specific user action may be
    /// unfunded. A simple "attack" we want to avoid is a cranker flooding the system
    /// with unnecessary price updates + cranks to continue making a profit off of
    /// "Completed" items.
    pub fn receives_crank_rewards(&self) -> bool {
        match self {
            CrankWorkInfo::CloseAllPositions { .. }
            | CrankWorkInfo::ResetLpBalances {}
            | CrankWorkInfo::Completed { .. } => false,
            CrankWorkInfo::Liquifunding { .. }
            | CrankWorkInfo::Liquidation { .. }
            | CrankWorkInfo::DeferredExec { .. }
            | CrankWorkInfo::LimitOrder { .. } => true,
        }
    }
}

/// Events related to the crank
pub mod events {
    use std::borrow::Cow;

    use super::*;
    use cosmwasm_std::Event;

    /// Batch processing of multiple cranks
    pub struct CrankExecBatchEvent {
        /// How many cranks were requested
        pub requested: u64,
        /// How many cranks were actually processed
        pub actual: Vec<CrankWorkInfo>,
    }

    impl PerpEvent for CrankExecBatchEvent {}
    impl From<CrankExecBatchEvent> for Event {
        fn from(CrankExecBatchEvent { requested, actual }: CrankExecBatchEvent) -> Self {
            let mut event = Event::new("crank-batch-exec")
                .add_attribute("requested", requested.to_string())
                .add_attribute("actual", actual.len().to_string());

            for (idx, work) in actual.into_iter().enumerate() {
                event = event.add_attribute(
                    format!("work-{}", idx + 1),
                    match work {
                        CrankWorkInfo::CloseAllPositions { .. } => {
                            Cow::Borrowed("close-all-positions")
                        }
                        CrankWorkInfo::ResetLpBalances {} => "reset-lp-balances".into(),
                        CrankWorkInfo::Liquifunding { position, .. } => {
                            format!("liquifund {position}").into()
                        }
                        CrankWorkInfo::Liquidation { position, .. } => {
                            format!("liquidation {position}").into()
                        }
                        CrankWorkInfo::DeferredExec {
                            deferred_exec_id, ..
                        } => format!("deferred exec {deferred_exec_id}").into(),
                        CrankWorkInfo::LimitOrder { order_id } => {
                            format!("limit order {order_id}").into()
                        }
                        CrankWorkInfo::Completed {
                            price_point_timestamp,
                        } => format!("completed {price_point_timestamp}").into(),
                    },
                )
            }

            event
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
                    CrankWorkInfo::DeferredExec { .. } => "deferred-exec",
                    CrankWorkInfo::LimitOrder { .. } => "limit-order",
                },
            );

            let (position_id, order_id, price_point_timestamp) = match src {
                CrankWorkInfo::CloseAllPositions {
                    position,
                    price_point,
                } => (Some(position), None, Some(price_point.timestamp)),
                CrankWorkInfo::ResetLpBalances {} => (None, None, None),
                CrankWorkInfo::Completed {
                    price_point_timestamp,
                } => (None, None, Some(price_point_timestamp)),
                CrankWorkInfo::Liquidation {
                    position,
                    liquidation_reason: _,
                    price_point,
                } => (Some(position), None, Some(price_point.timestamp)),
                CrankWorkInfo::Liquifunding { position } => (Some(position), None, None),
                CrankWorkInfo::DeferredExec {
                    deferred_exec_id: _,
                    target,
                    price_point_timestamp,
                } => (
                    target.position_id(),
                    target.order_id(),
                    Some(price_point_timestamp),
                ),
                CrankWorkInfo::LimitOrder { order_id } => (None, Some(order_id), None),
            };

            if let Some(position_id) = position_id {
                event = event.add_attribute("pos-id", position_id.to_string());
            }
            if let Some(price_point_timestamp) = price_point_timestamp {
                event =
                    event.add_attribute("price-point-timestamp", price_point_timestamp.to_string());
            }

            if let CrankWorkInfo::Liquidation {
                price_point,
                liquidation_reason,
                ..
            } = src
            {
                event = event
                    .add_attribute("price-point", serde_json::to_string(&price_point).unwrap())
                    .add_attribute("liquidation-reason", liquidation_reason.to_string());
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
            let get_price_point = || evt.json_attr("price-point");

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
                    price_point: get_price_point()?,
                }),
                "limit-order" => Ok(CrankWorkInfo::LimitOrder {
                    order_id: OrderId::new(evt.u64_attr("order-id")?),
                }),
                _ => Err(PerpError::unimplemented().into()),
            })
        }
    }
}
