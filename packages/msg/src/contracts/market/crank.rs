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
    },
    /// Deferred execution (open/update/closed) can be executed.
    DeferredExec {
        /// ID to be processed
        deferred_exec_id: DeferredExecId,
        /// Target of the action
        target: DeferredExecTarget,
    },
    /// Limit order can be opened
    LimitOrder {
        /// ID of the order to be opened
        order_id: OrderId,
    },
    /// Finished all processing for a given price update
    Completed {},
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
        pub actual: Vec<(CrankWorkInfo, PricePoint)>,
    }

    impl PerpEvent for CrankExecBatchEvent {}
    impl From<CrankExecBatchEvent> for Event {
        fn from(CrankExecBatchEvent { requested, actual }: CrankExecBatchEvent) -> Self {
            let mut event = Event::new("crank-batch-exec")
                .add_attribute("requested", requested.to_string())
                .add_attribute("actual", actual.len().to_string());

            for (idx, (work, price_point)) in actual.into_iter().enumerate() {
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
                        CrankWorkInfo::LimitOrder { order_id, .. } => {
                            format!("limit order {order_id}").into()
                        }
                        CrankWorkInfo::Completed {} => {
                            format!("completed {}", price_point.timestamp).into()
                        }
                    },
                )
            }

            event
        }
    }

    /// CrankWorkInfo with a given price point
    pub struct CrankWorkInfoEvent {
        /// The work info itself
        pub work_info: CrankWorkInfo,
        /// A price point, i.e. the price point that triggered the work
        pub price_point: PricePoint,
    }

    impl PerpEvent for CrankWorkInfoEvent {}
    impl From<CrankWorkInfoEvent> for Event {
        fn from(
            CrankWorkInfoEvent {
                work_info,
                price_point,
            }: CrankWorkInfoEvent,
        ) -> Self {
            let mut event = Event::new("crank-work")
                .add_attribute(
                    "kind",
                    match work_info {
                        CrankWorkInfo::CloseAllPositions { .. } => "close-all-positions",
                        CrankWorkInfo::ResetLpBalances { .. } => "reset-lp-balances",
                        CrankWorkInfo::Completed { .. } => "completed",
                        CrankWorkInfo::Liquidation { .. } => "liquidation",
                        CrankWorkInfo::Liquifunding { .. } => "liquifunding",
                        CrankWorkInfo::DeferredExec { .. } => "deferred-exec",
                        CrankWorkInfo::LimitOrder { .. } => "limit-order",
                    },
                )
                // Keeping this for backwards-compat with indexer, though it should be deprecated
                // and deserialized from price-point itself
                .add_attribute("price-point-timestamp", price_point.timestamp.to_string())
                .add_attribute("price-point", serde_json::to_string(&price_point).unwrap());

            let (position_id, order_id) = match work_info {
                CrankWorkInfo::CloseAllPositions { position } => (Some(position), None),
                CrankWorkInfo::ResetLpBalances {} => (None, None),
                CrankWorkInfo::Completed {} => (None, None),
                CrankWorkInfo::Liquidation {
                    position,
                    liquidation_reason: _,
                } => (Some(position), None),
                CrankWorkInfo::Liquifunding { position } => (Some(position), None),
                CrankWorkInfo::DeferredExec {
                    deferred_exec_id: _,
                    target,
                } => (target.position_id(), target.order_id()),
                CrankWorkInfo::LimitOrder { order_id } => (None, Some(order_id)),
            };

            if let Some(position_id) = position_id {
                event = event.add_attribute("pos-id", position_id.to_string());
            }

            if let CrankWorkInfo::Liquidation {
                liquidation_reason, ..
            } = work_info
            {
                event = event.add_attribute("liquidation-reason", liquidation_reason.to_string());
            }

            if let CrankWorkInfo::DeferredExec {
                deferred_exec_id,
                target,
            } = work_info
            {
                event = event
                    .add_attribute("deferred-exec-id", deferred_exec_id.to_string())
                    .add_attribute(
                        "deferred-exec-target",
                        match target {
                            DeferredExecTarget::DoesNotExist => "not-exist",
                            DeferredExecTarget::Position { .. } => "position",
                            DeferredExecTarget::Order { .. } => "order",
                        },
                    );
            }

            if let Some(order_id) = order_id {
                event = event.add_attribute("order-id", order_id.to_string());
            }

            event
        }
    }

    impl TryFrom<Event> for CrankWorkInfoEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            let get_position_id =
                || -> anyhow::Result<PositionId> { Ok(PositionId::new(evt.u64_attr("pos-id")?)) };
            let get_order_id =
                || -> anyhow::Result<OrderId> { Ok(OrderId::new(evt.u64_attr("order-id")?)) };

            let get_liquidation_reason = || -> anyhow::Result<LiquidationReason> {
                match evt.string_attr("liquidation-reason")?.as_str() {
                    "liquidated" => Ok(LiquidationReason::Liquidated),
                    "take-profit" => Ok(LiquidationReason::MaxGains),
                    _ => Err(PerpError::unimplemented().into()),
                }
            };

            let work_info = evt.map_attr_result("kind", |s| match s {
                "completed" => Ok(CrankWorkInfo::Completed {}),
                "liquifunding" => Ok(CrankWorkInfo::Liquifunding {
                    position: get_position_id()?,
                }),
                "liquidation" => Ok(CrankWorkInfo::Liquidation {
                    position: get_position_id()?,
                    liquidation_reason: get_liquidation_reason()?,
                }),
                "limit-order" => Ok(CrankWorkInfo::LimitOrder {
                    order_id: get_order_id()?,
                }),
                "close-all-positions" => Ok(CrankWorkInfo::CloseAllPositions {
                    position: get_position_id()?,
                }),
                "reset-lp-balances" => Ok(CrankWorkInfo::ResetLpBalances {}),
                "deferred-exec" => Ok(CrankWorkInfo::DeferredExec {
                    deferred_exec_id: DeferredExecId::from_u64(evt.u64_attr("deferred-exec-id")?),
                    target: evt.map_attr_result(
                        "deferred-exec-target",
                        |x| -> Result<DeferredExecTarget> {
                            match x {
                                "not-exist" => Ok(DeferredExecTarget::DoesNotExist),
                                "position" => get_position_id().map(DeferredExecTarget::Position),
                                "order" => get_order_id().map(DeferredExecTarget::Order),
                                _ => Err(PerpError::unimplemented().into()),
                            }
                        },
                    )?,
                }),

                _ => Err(PerpError::unimplemented().into()),
            })?;

            Ok(Self {
                work_info,
                price_point: evt.json_attr("price-point")?,
            })
        }
    }

    // Not sure if this is needed anymore, but keeping it in for backwards compat at least
    impl TryFrom<Event> for CrankWorkInfo {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            CrankWorkInfoEvent::try_from(evt).map(|x| x.work_info)
        }
    }
}
