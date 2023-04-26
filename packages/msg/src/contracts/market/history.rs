//! History data types
use shared::prelude::*;

/// History events
pub mod events {
    use super::*;
    use crate::constants::event_key;
    use crate::contracts::market::entry::{
        LpAction, LpActionKind, PositionAction, PositionActionKind,
    };
    use crate::contracts::market::position::PositionId;

    /// Trade volume increased by the given amount
    pub struct TradeVolumeEvent {
        /// Additional trade volume
        pub volume_usd: Usd,
    }

    impl PerpEvent for TradeVolumeEvent {}
    impl From<TradeVolumeEvent> for Event {
        fn from(src: TradeVolumeEvent) -> Self {
            Event::new("history-trade-volume")
                .add_attribute("volume-usd", src.volume_usd.to_string())
        }
    }
    impl TryFrom<Event> for TradeVolumeEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(TradeVolumeEvent {
                volume_usd: evt.decimal_attr("volume-usd")?,
            })
        }
    }

    /// Realized PnL
    pub struct PnlEvent {
        /// In collateral
        pub pnl: Signed<Collateral>,
        /// In USD
        pub pnl_usd: Signed<Usd>,
    }

    impl PerpEvent for PnlEvent {}
    impl From<PnlEvent> for Event {
        fn from(src: PnlEvent) -> Self {
            Event::new("history-pnl")
                .add_attribute("pnl", src.pnl.to_string())
                .add_attribute("pnl-usd", src.pnl_usd.to_string())
        }
    }
    impl TryFrom<Event> for PnlEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(PnlEvent {
                pnl: evt.number_attr("pnl")?,
                pnl_usd: evt.number_attr("pnl-usd")?,
            })
        }
    }

    /// Action taken on a position
    pub struct PositionActionEvent {
        /// Which position
        pub pos_id: PositionId,
        /// Action
        pub action: PositionAction,
    }

    impl PerpEvent for PositionActionEvent {}
    impl From<PositionActionEvent> for Event {
        fn from(src: PositionActionEvent) -> Self {
            let evt = Event::new("history-position-action")
                .add_attribute(event_key::POS_ID, src.pos_id.to_string())
                .add_attribute(
                    event_key::POSITION_ACTION_KIND,
                    match src.action.kind {
                        PositionActionKind::Open => "open",
                        PositionActionKind::Update => "update",
                        PositionActionKind::Close => "close",
                        PositionActionKind::Transfer => "transfer",
                    },
                )
                .add_attribute(
                    event_key::POSITION_ACTION_TIMESTAMP,
                    src.action.timestamp.to_string(),
                )
                .add_attribute(
                    event_key::POSITION_ACTION_COLLATERAL,
                    src.action.collateral.to_string(),
                );

            let evt = match src.action.leverage {
                Some(leverage) => {
                    evt.add_attribute(event_key::POSITION_ACTION_LEVERAGE, leverage.to_string())
                }
                None => evt,
            };
            let evt = match src.action.max_gains {
                Some(max_gains) => {
                    evt.add_attribute(event_key::POSITION_ACTION_MAX_GAINS, max_gains.to_string())
                }
                None => evt,
            };

            let evt = match src.action.trade_fee {
                None => evt,
                Some(trade_fee) => {
                    evt.add_attribute(event_key::POSITION_ACTION_TRADE_FEE, trade_fee.to_string())
                }
            };

            let evt = match src.action.delta_neutrality_fee {
                None => evt,
                Some(delta_neutrality_fee) => evt.add_attribute(
                    event_key::POSITION_ACTION_DELTA_NEUTRALITY_FEE,
                    delta_neutrality_fee.to_string(),
                ),
            };

            let evt = match src.action.old_owner {
                None => evt,
                Some(old_owner) => evt.add_attribute(
                    event_key::POSITION_ACTION_OLD_OWNER,
                    old_owner.into_string(),
                ),
            };

            match src.action.new_owner {
                None => evt,
                Some(new_owner) => evt.add_attribute(
                    event_key::POSITION_ACTION_NEW_OWNER,
                    new_owner.into_string(),
                ),
            }
        }
    }

    impl TryFrom<Event> for PositionActionEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(PositionActionEvent {
                pos_id: PositionId::new(evt.u64_attr(event_key::POS_ID)?),
                action: PositionAction {
                    kind: evt.map_attr_result(event_key::POSITION_ACTION_KIND, |s| match s {
                        "open" => Ok(PositionActionKind::Open),
                        "update" => Ok(PositionActionKind::Update),
                        "close" => Ok(PositionActionKind::Close),
                        "transfer" => Ok(PositionActionKind::Transfer),
                        _ => Err(PerpError::unimplemented().into()),
                    })?,
                    timestamp: evt.timestamp_attr(event_key::POSITION_ACTION_TIMESTAMP)?,
                    collateral: evt.decimal_attr(event_key::POSITION_ACTION_COLLATERAL)?,
                    leverage: evt.try_leverage_to_base_attr(event_key::POSITION_ACTION_LEVERAGE)?,
                    max_gains: evt
                        .try_map_attr(event_key::POSITION_ACTION_MAX_GAINS, |value| {
                            MaxGainsInQuote::try_from(value)
                        })
                        .transpose()?,
                    trade_fee: evt.try_decimal_attr(event_key::POSITION_ACTION_TRADE_FEE)?,
                    delta_neutrality_fee: evt
                        .try_number_attr(event_key::POSITION_ACTION_DELTA_NEUTRALITY_FEE)?,
                    old_owner: evt.try_unchecked_addr_attr(event_key::POSITION_ACTION_OLD_OWNER)?,
                    new_owner: evt.try_unchecked_addr_attr(event_key::POSITION_ACTION_NEW_OWNER)?,
                },
            })
        }
    }

    /// Event when a new action is added to the liquidity provider history.
    pub struct LpActionEvent {
        /// Liquidity provider
        pub addr: Addr,
        /// Action that occurred
        pub action: LpAction,
        /// Identifier for the action
        pub action_id: u64,
    }

    impl PerpEvent for LpActionEvent {}
    impl From<LpActionEvent> for Event {
        fn from(src: LpActionEvent) -> Self {
            let event = Event::new("history-lp-action")
                .add_attribute(event_key::LP_ACTION_ADDRESS, src.addr.to_string())
                .add_attribute(event_key::LP_ACTION_ID, src.action_id.to_string())
                .add_attribute(
                    event_key::LP_ACTION_KIND,
                    match src.action.kind {
                        LpActionKind::DepositLp => "deposit-lp",
                        LpActionKind::DepositXlp => "deposit-xlp",
                        LpActionKind::ReinvestYieldLp => "reinvest-yield-lp",
                        LpActionKind::ReinvestYieldXlp => "reinvest-yield-xlp",
                        LpActionKind::UnstakeXlp => "unstake-xlp",
                        LpActionKind::Withdraw => "withdraw",
                        LpActionKind::ClaimYield => "claim-yield",
                        LpActionKind::CollectLp => "collect-lp",
                    },
                )
                .add_attribute(
                    event_key::LP_ACTION_TIMESTAMP,
                    src.action.timestamp.to_string(),
                )
                .add_attribute(
                    event_key::LP_ACTION_COLLATERAL,
                    src.action.collateral.to_string(),
                )
                .add_attribute(
                    event_key::LP_ACTION_COLLATERAL_USD,
                    src.action.collateral_usd.to_string(),
                );
            match src.action.tokens {
                Some(tokens) => {
                    event.add_attribute(event_key::LP_ACTION_TOKENS, tokens.to_string())
                }
                None => event,
            }
        }
    }

    impl TryFrom<Event> for LpActionEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(LpActionEvent {
                addr: evt.unchecked_addr_attr(event_key::LP_ACTION_ADDRESS)?,
                action_id: evt.u64_attr(event_key::LP_ACTION_ID)?,
                action: LpAction {
                    kind: evt.map_attr_result(event_key::LP_ACTION_KIND, |s| match s {
                        "deposit-lp" => Ok(LpActionKind::DepositLp),
                        "deposit-xlp" => Ok(LpActionKind::DepositXlp),
                        "reinvest-yield-lp" => Ok(LpActionKind::ReinvestYieldLp),
                        "reinvest-yield-xlp" => Ok(LpActionKind::ReinvestYieldXlp),
                        "unstake-xlp" => Ok(LpActionKind::UnstakeXlp),
                        "withdraw" => Ok(LpActionKind::Withdraw),
                        "claim-yield" => Ok(LpActionKind::ClaimYield),
                        "collect-lp" => Ok(LpActionKind::CollectLp),
                        _ => Err(PerpError::unimplemented().into()),
                    })?,
                    timestamp: evt.timestamp_attr(event_key::LP_ACTION_TIMESTAMP)?,
                    tokens: evt.try_decimal_attr(event_key::LP_ACTION_TOKENS)?,
                    collateral: evt.decimal_attr(event_key::LP_ACTION_COLLATERAL)?,
                    collateral_usd: evt.decimal_attr(event_key::LP_ACTION_COLLATERAL_USD)?,
                },
            })
        }
    }

    /// Liquidity deposited into the pool
    pub struct LpDepositEvent {
        /// Deposited amount in collateral
        pub deposit: Collateral,
        /// Current value of deposit in USD
        pub deposit_usd: Usd,
    }

    impl PerpEvent for LpDepositEvent {}
    impl From<LpDepositEvent> for Event {
        fn from(src: LpDepositEvent) -> Self {
            Event::new("history-lp-deposit")
                .add_attribute("deposit", src.deposit.to_string())
                .add_attribute("deposit-usd", src.deposit_usd.to_string())
        }
    }
    impl TryFrom<Event> for LpDepositEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(LpDepositEvent {
                deposit: evt.decimal_attr("deposit")?,
                deposit_usd: evt.decimal_attr("deposit-usd")?,
            })
        }
    }

    /// LP yield was claimed
    pub struct LpYieldEvent {
        /// Liquidity provider
        pub addr: Addr,
        /// Yield amount in collateral
        pub r#yield: Collateral,
        /// Yield value in USD at current rate
        pub yield_usd: Usd,
    }

    impl PerpEvent for LpYieldEvent {}
    impl From<LpYieldEvent> for Event {
        fn from(
            LpYieldEvent {
                addr,
                r#yield,
                yield_usd,
            }: LpYieldEvent,
        ) -> Self {
            Event::new("history-lp-yield")
                .add_attribute("addr", addr.to_string())
                .add_attribute("yield", r#yield.to_string())
                .add_attribute("yield-usd", yield_usd.to_string())
        }
    }
    impl TryFrom<Event> for LpYieldEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(LpYieldEvent {
                addr: evt.unchecked_addr_attr("addr")?,
                r#yield: evt.decimal_attr("yield")?,
                yield_usd: evt.decimal_attr("yield-usd")?,
            })
        }
    }
}
