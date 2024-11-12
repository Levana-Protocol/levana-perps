// CosmWasm 1.0 redacts submessage errors, turning them all into "error code 5"
// This will be changed in Cosmwasm 2.0, but for now, we need to recover errors manually
//
// The validation branch tries to follow the execution branch as closely as possible
// by way of shared functions and structs that contain as much of the validation as possible in their construction
//
// The execution branch then goes one step further - applying the changes to the state, using that same struct
// In order to ensure that the struct is used, we annotate with `#[must_use]` and either `apply()` or `discard()` are called
use perpswap::compat::BackwardsCompatTakeProfit;
use perpswap::contracts::market::{
    deferred_execution::{
        DeferredExecCompleteTarget, DeferredExecId, DeferredExecItem, DeferredExecWithStatus,
    },
    entry::SlippageAssert,
    position::{events::PositionSaveReason, CollateralAndUsd},
};

use crate::state::{
    order::{CancelLimitOrderExec, PlaceLimitOrderExec},
    position::{
        close::ClosePositionExec,
        liquifund::PositionLiquifund,
        update::{
            TriggerOrderExec, UpdatePositionCollateralExec, UpdatePositionLeverageExec,
            UpdatePositionMaxGainsExec, UpdatePositionSizeExec, UpdatePositionStopLossPriceExec,
            UpdatePositionTakeProfitPriceExec,
        },
        OpenPositionExec, OpenPositionParams,
    },
};
use crate::{prelude::*, state::position::get_position};

impl State<'_> {
    pub(crate) fn perform_deferred_exec(
        &self,
        ctx: &mut StateContext,
        id: DeferredExecId,
        price_point_timestamp: Timestamp,
    ) -> Result<()> {
        let price_point = self.spot_price(ctx.storage, price_point_timestamp)?;
        debug_assert!(price_point.timestamp == price_point_timestamp);
        let item = self.load_deferred_exec_item(ctx.storage, id)?;
        let pos_order_id = helper_execute(self, ctx, item.clone(), price_point)?;
        self.mark_deferred_exec_success(ctx, item, pos_order_id)?;
        Ok(())
    }

    pub(crate) fn deferred_validate(
        &self,
        store: &dyn Storage,
        id: DeferredExecId,
        price_point: &PricePoint,
        slippage_check: SlippageCheckStatus,
    ) -> Result<()> {
        let item = self.load_deferred_exec_item(store, id)?;
        helper_validate(self, store, item, price_point, slippage_check)
    }
}

pub enum SlippageCheckStatus {
    SlippageCheck,
    NoSlippageCheck,
}

fn helper_execute(
    state: &State,
    ctx: &mut StateContext,
    item: DeferredExecWithStatus,
    price_point: PricePoint,
) -> Result<DeferredExecCompleteTarget> {
    match item.item {
        // TODO: remove this once the deprecated fields are fully removed
        #[allow(deprecated)]
        DeferredExecItem::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit,
            amount,
            crank_fee,
            crank_fee_usd,
        } => {
            // eventually this will be deprecated - see BackwardsCompatTakeProfit notes for details
            let take_profit_trader = match (take_profit, max_gains) {
                (None, None) => {
                    bail!("must supply at least one of take_profit or max_gains");
                }
                (Some(take_profit_price), None) => take_profit_price,
                (take_profit, Some(max_gains)) => {
                    let take_profit = match take_profit {
                        None => None,
                        Some(take_profit) => match take_profit {
                            TakeProfitTrader::PosInfinity => {
                                bail!("cannot set infinite take profit price and max_gains")
                            }
                            TakeProfitTrader::Finite(x) => Some(PriceBaseInQuote::from_non_zero(x)),
                        },
                    };
                    BackwardsCompatTakeProfit {
                        collateral: amount,
                        market_type: state.market_id(ctx.storage)?.get_market_type(),
                        direction,
                        leverage,
                        max_gains,
                        take_profit,
                        price_point: &price_point,
                    }
                    .calc()?
                }
            };

            OpenPositionExec::new(
                state,
                ctx.storage,
                OpenPositionParams {
                    owner: item.owner,
                    collateral: amount,
                    leverage,
                    direction,
                    slippage_assert,
                    stop_loss_override,
                    take_profit_trader,
                    crank_fee: CollateralAndUsd::from_pair(crank_fee, crank_fee_usd),
                },
                &price_point,
            )?
            .apply(state, ctx, PositionSaveReason::OpenMarket)
            .map(DeferredExecCompleteTarget::Position)
        }
        DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { id, amount } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            UpdatePositionCollateralExec::new(
                state,
                ctx.storage,
                get_position(ctx.storage, id)?,
                amount.into_signed(),
                &price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionAddCollateralImpactSize {
            id,
            slippage_assert,
            amount,
        } => {
            let funds = amount.into_signed();
            let notional_size = state.update_size_new_notional_size(ctx.storage, id, funds)?;
            execute_slippage_assert_and_liquifund(
                state,
                ctx,
                id,
                Some(notional_size),
                slippage_assert,
                &price_point,
            )?;
            UpdatePositionSizeExec::new(
                state,
                ctx.storage,
                get_position(ctx.storage, id)?,
                funds,
                &price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { id, amount } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            UpdatePositionCollateralExec::new(
                state,
                ctx.storage,
                get_position(ctx.storage, id)?,
                -amount.into_signed(),
                &price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactSize {
            id,
            amount,
            slippage_assert,
        } => {
            let funds = -amount.into_signed();
            let notional_size = state.update_size_new_notional_size(ctx.storage, id, funds)?;
            execute_slippage_assert_and_liquifund(
                state,
                ctx,
                id,
                Some(notional_size),
                slippage_assert,
                &price_point,
            )?;
            UpdatePositionSizeExec::new(
                state,
                ctx.storage,
                get_position(ctx.storage, id)?,
                funds,
                &price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionLeverage {
            id,
            leverage,
            slippage_assert,
        } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            let notional_size =
                state.update_leverage_new_notional_size(ctx.storage, id, leverage, &price_point)?;
            if let Some(slippage_assert) = slippage_assert {
                let market_type = state.market_id(ctx.storage)?.get_market_type();
                let pos = get_position(ctx.storage, id)?;
                let delta_notional_size = (notional_size - pos.notional_size)?;
                state.do_slippage_assert(
                    ctx.storage,
                    slippage_assert,
                    delta_notional_size,
                    market_type,
                    Some(pos.liquidation_margin.delta_neutrality),
                    &price_point,
                )?;
            }
            UpdatePositionLeverageExec::new(
                state,
                ctx.storage,
                get_position(ctx.storage, id)?,
                notional_size,
                &price_point,
            )?
            .apply(state, ctx)?;

            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionMaxGains { id, max_gains } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            UpdatePositionMaxGainsExec::new(
                state,
                ctx.storage,
                get_position(ctx.storage, id)?,
                max_gains,
                &price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionTakeProfitPrice {
            id,
            price: take_profit_price,
        } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            UpdatePositionTakeProfitPriceExec::new(
                state,
                ctx.storage,
                get_position(ctx.storage, id)?,
                take_profit_price,
                &price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionStopLossPrice { id, stop_loss } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            UpdatePositionStopLossPriceExec::new(
                state,
                ctx.storage,
                id,
                stop_loss,
                price_point,
                false,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::ClosePosition {
            id,
            slippage_assert,
        } => {
            helper_close_position(state, ctx.storage, id, slippage_assert, price_point)?
                .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::SetTriggerOrder {
            id,
            stop_loss_override,
            take_profit,
        } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            TriggerOrderExec::new(
                state,
                ctx.storage,
                id,
                stop_loss_override,
                take_profit,
                price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }

        // TODO: remove this once the deprecated fields are fully removed
        #[allow(deprecated)]
        DeferredExecItem::PlaceLimitOrder {
            trigger_price,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit,
            amount,
            crank_fee,
            crank_fee_usd,
        } => {
            // eventually this will be deprecated - see BackwardsCompatTakeProfit notes for details
            let take_profit_price = match (take_profit, max_gains) {
                (None, None) => {
                    bail!("must supply at least one of take_profit or max_gains");
                }
                (Some(take_profit_price), None) => take_profit_price,
                (take_profit, Some(max_gains)) => {
                    let take_profit = match take_profit {
                        None => None,
                        Some(take_profit) => match take_profit {
                            TakeProfitTrader::PosInfinity => {
                                bail!("cannot set infinite take profit price and max_gains")
                            }
                            TakeProfitTrader::Finite(x) => Some(PriceBaseInQuote::from_non_zero(x)),
                        },
                    };
                    BackwardsCompatTakeProfit {
                        collateral: amount,
                        market_type: state.market_id(ctx.storage)?.get_market_type(),
                        direction,
                        leverage,
                        max_gains,
                        take_profit,
                        price_point: &price_point,
                    }
                    .calc()?
                }
            };

            let order_id = PlaceLimitOrderExec::new(
                state,
                ctx.storage,
                item.owner,
                trigger_price,
                amount,
                leverage,
                direction.into_notional(state.market_type(ctx.storage)?),
                stop_loss_override,
                take_profit_price,
                crank_fee,
                crank_fee_usd,
                price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Order(order_id))
        }
        DeferredExecItem::CancelLimitOrder { order_id } => {
            CancelLimitOrderExec::new(ctx.storage, order_id)?.apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Order(order_id))
        }
    }
}

fn execute_slippage_assert_and_liquifund(
    state: &State,
    ctx: &mut StateContext,
    id: PositionId,
    notional_size: Option<Signed<Notional>>,
    slippage_assert: Option<SlippageAssert>,
    price_point: &PricePoint,
) -> Result<()> {
    // We used to assert position owner here, but that's now handled when queueing the deferred message.

    update_position_slippage_assert(
        state,
        ctx.storage,
        id,
        notional_size,
        slippage_assert,
        price_point,
    )?;

    let pos = get_position(ctx.storage, id)?;

    debug_assert!(pos.next_liquifunding >= price_point.timestamp);

    let starts_at = pos.liquifunded_at;
    state.position_liquifund_store(
        ctx,
        pos,
        starts_at,
        price_point.timestamp,
        false,
        PositionSaveReason::Update,
    )?;

    Ok(())
}

fn helper_validate(
    state: &State,
    store: &dyn Storage,
    item: DeferredExecWithStatus,
    price_point: &PricePoint,
    slippage_check: SlippageCheckStatus,
) -> Result<()> {
    match item.item {
        // TODO: remove this once the deprecated fields are fully removed
        #[allow(deprecated)]
        DeferredExecItem::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit,
            amount,
            crank_fee,
            crank_fee_usd,
        } => {
            // if the status of DeferredExecItem is Pending, avoid validating for slippage_assert
            let slippage_assert = match slippage_check {
                SlippageCheckStatus::SlippageCheck => slippage_assert,
                SlippageCheckStatus::NoSlippageCheck => None,
            };
            // eventually this will be deprecated - see BackwardsCompatTakeProfit notes for details
            let take_profit_trader = match (take_profit, max_gains) {
                (None, None) => {
                    bail!("must supply at least one of take_profit or max_gains");
                }
                (Some(take_profit_price), None) => take_profit_price,
                (take_profit, Some(max_gains)) => {
                    let take_profit = match take_profit {
                        None => None,
                        Some(take_profit) => match take_profit {
                            TakeProfitTrader::PosInfinity => {
                                bail!("cannot set infinite take profit price and max_gains")
                            }
                            TakeProfitTrader::Finite(x) => Some(PriceBaseInQuote::from_non_zero(x)),
                        },
                    };
                    BackwardsCompatTakeProfit {
                        collateral: amount,
                        market_type: state.market_id(store)?.get_market_type(),
                        direction,
                        leverage,
                        max_gains,
                        take_profit,
                        price_point,
                    }
                    .calc()?
                }
            };

            OpenPositionExec::new(
                state,
                store,
                OpenPositionParams {
                    owner: item.owner,
                    collateral: amount,
                    leverage,
                    direction,
                    slippage_assert,
                    stop_loss_override,
                    take_profit_trader,
                    crank_fee: CollateralAndUsd::from_pair(crank_fee, crank_fee_usd),
                },
                price_point,
            )?
            .discard();

            Ok(())
        }
        DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { id, amount } => {
            let liquifund =
                validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?;

            UpdatePositionCollateralExec::new(
                state,
                store,
                liquifund.position.into(),
                amount.into_signed(),
                price_point,
            )?
            .discard();
            Ok(())
        }
        DeferredExecItem::UpdatePositionAddCollateralImpactSize {
            id,
            slippage_assert,
            amount,
        } => {
            let funds = amount.into_signed();
            // if the status of DeferredExecItem is Pending, avoid validating for slippage_assert
            let slippage_assert = match slippage_check {
                SlippageCheckStatus::SlippageCheck => slippage_assert,
                SlippageCheckStatus::NoSlippageCheck => None,
            };

            let notional_size = state.update_size_new_notional_size(store, id, funds)?;
            let liquifund = validate_slippage_assert_and_liquifund(
                state,
                store,
                id,
                Some(notional_size),
                slippage_assert,
                price_point,
            )?;
            UpdatePositionSizeExec::new(
                state,
                store,
                liquifund.position.into(),
                funds,
                price_point,
            )?
            .discard();

            Ok(())
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { id, amount } => {
            let liquifund =
                validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?;
            UpdatePositionCollateralExec::new(
                state,
                store,
                liquifund.position.into(),
                -amount.into_signed(),
                price_point,
            )?
            .discard();
            Ok(())
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactSize {
            id,
            amount,
            slippage_assert,
        } => {
            let funds = -amount.into_signed();
            let notional_size = state.update_size_new_notional_size(store, id, funds)?;
            // if the status of DeferredExecItem is Pending, avoid validating for slippage_assert
            let slippage_assert = match slippage_check {
                SlippageCheckStatus::SlippageCheck => slippage_assert,
                SlippageCheckStatus::NoSlippageCheck => None,
            };

            let liquifund = validate_slippage_assert_and_liquifund(
                state,
                store,
                id,
                Some(notional_size),
                slippage_assert,
                price_point,
            )?;
            UpdatePositionSizeExec::new(
                state,
                store,
                liquifund.position.into(),
                funds,
                price_point,
            )?
            .discard();
            Ok(())
        }
        DeferredExecItem::UpdatePositionLeverage {
            id,
            leverage,
            slippage_assert,
        } => {
            // if the status of DeferredExecItem is Pending, avoid validating for slippage_assert
            let slippage_assert = match slippage_check {
                SlippageCheckStatus::SlippageCheck => slippage_assert,
                SlippageCheckStatus::NoSlippageCheck => None,
            };

            let liquifund = validate_slippage_assert_and_liquifund(
                state,
                store,
                id,
                None,
                slippage_assert.clone(),
                price_point,
            )?;
            let notional_size =
                state.update_leverage_new_notional_size(store, id, leverage, price_point)?;

            let pos: Position = liquifund.position.into();

            if let Some(slippage_assert) = slippage_assert {
                let market_type = state.market_id(store)?.get_market_type();

                let delta_notional_size = (notional_size - pos.notional_size)?;
                state.do_slippage_assert(
                    store,
                    slippage_assert,
                    delta_notional_size,
                    market_type,
                    Some(pos.liquidation_margin.delta_neutrality),
                    price_point,
                )?;
            }

            UpdatePositionLeverageExec::new(state, store, pos, notional_size, price_point)?
                .discard();
            Ok(())
        }
        DeferredExecItem::UpdatePositionMaxGains { id, max_gains } => {
            let liquifund =
                validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?;
            UpdatePositionMaxGainsExec::new(
                state,
                store,
                liquifund.position.into(),
                max_gains,
                price_point,
            )?
            .discard();
            Ok(())
        }
        DeferredExecItem::UpdatePositionTakeProfitPrice {
            id,
            price: take_profit_price,
        } => {
            let liquifund =
                validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?;
            UpdatePositionTakeProfitPriceExec::new(
                state,
                store,
                liquifund.position.into(),
                take_profit_price,
                price_point,
            )?
            .discard();
            Ok(())
        }
        DeferredExecItem::UpdatePositionStopLossPrice { id, stop_loss } => {
            validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?
                .discard();
            UpdatePositionStopLossPriceExec::new(state, store, id, stop_loss, *price_point, true)?
                .discard();
            Ok(())
        }
        DeferredExecItem::ClosePosition {
            id,
            slippage_assert,
        } => {
            // if the status of DeferredExecItem is Pending, avoid validating for slippage_assert
            let slippage_assert = match slippage_check {
                SlippageCheckStatus::SlippageCheck => slippage_assert,
                SlippageCheckStatus::NoSlippageCheck => None,
            };

            helper_close_position(state, store, id, slippage_assert, *price_point)?.discard();
            Ok(())
        }
        DeferredExecItem::SetTriggerOrder {
            id,
            stop_loss_override,
            take_profit,
        } => {
            validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?
                .discard();
            TriggerOrderExec::new(
                state,
                store,
                id,
                stop_loss_override,
                take_profit,
                *price_point,
            )?
            .discard();
            Ok(())
        }

        // TODO: remove this once the deprecated fields are fully removed
        #[allow(deprecated)]
        DeferredExecItem::PlaceLimitOrder {
            trigger_price,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit,
            amount,
            crank_fee,
            crank_fee_usd,
        } => {
            // eventually this will be deprecated - see BackwardsCompatTakeProfit notes for details
            let take_profit_price = match (take_profit, max_gains) {
                (None, None) => {
                    bail!("must supply at least one of take_profit or max_gains");
                }
                (Some(take_profit_price), None) => take_profit_price,
                (take_profit, Some(max_gains)) => {
                    let take_profit = match take_profit {
                        None => None,
                        Some(take_profit) => match take_profit {
                            TakeProfitTrader::PosInfinity => {
                                bail!("cannot set infinite take profit price and max_gains")
                            }
                            TakeProfitTrader::Finite(x) => Some(PriceBaseInQuote::from_non_zero(x)),
                        },
                    };
                    BackwardsCompatTakeProfit {
                        collateral: amount,
                        market_type: state.market_id(store)?.get_market_type(),
                        direction,
                        leverage,
                        max_gains,
                        take_profit,
                        price_point,
                    }
                    .calc()?
                }
            };
            PlaceLimitOrderExec::new(
                state,
                store,
                item.owner,
                trigger_price,
                amount,
                leverage,
                direction.into_notional(state.market_type(store)?),
                stop_loss_override,
                take_profit_price,
                crank_fee,
                crank_fee_usd,
                *price_point,
            )?
            .discard();
            Ok(())
        }
        DeferredExecItem::CancelLimitOrder { order_id } => {
            CancelLimitOrderExec::new(store, order_id)?.discard();
            Ok(())
        }
    }
}

fn validate_slippage_assert_and_liquifund(
    state: &State,
    store: &dyn Storage,
    id: PositionId,
    notional_size: Option<Signed<Notional>>,
    slippage_assert: Option<SlippageAssert>,
    price_point: &PricePoint,
) -> Result<PositionLiquifund> {
    update_position_slippage_assert(
        state,
        store,
        id,
        notional_size,
        slippage_assert,
        price_point,
    )?;

    let pos = get_position(store, id)?;

    debug_assert!(pos.next_liquifunding >= price_point.timestamp);

    let starts_at = pos.liquifunded_at;
    PositionLiquifund::new(state, store, pos, starts_at, price_point.timestamp, false)
}

fn update_position_slippage_assert(
    state: &State,
    store: &dyn Storage,
    id: PositionId,
    notional_size: Option<Signed<Notional>>,
    slippage_assert: Option<SlippageAssert>,
    price_point: &PricePoint,
) -> Result<()> {
    if let Some(slippage_assert) = slippage_assert {
        let market_type = state.market_id(store)?.get_market_type();
        let pos = get_position(store, id)?;
        let delta_notional_size = (notional_size.unwrap_or(pos.notional_size) - pos.notional_size)?;
        state.do_slippage_assert(
            store,
            slippage_assert,
            delta_notional_size,
            market_type,
            Some(pos.liquidation_margin.delta_neutrality),
            price_point,
        )?;
    }

    Ok(())
}

fn helper_close_position(
    state: &State,
    store: &dyn Storage,
    id: PositionId,
    slippage_assert: Option<SlippageAssert>,
    price_point: PricePoint,
) -> Result<ClosePositionExec> {
    let pos = match get_position(store, id) {
        Ok(pos) => Ok(pos),
        Err(e) => match state.load_closed_position(store, id) {
            Ok(Some(closed)) => Err(MarketError::PositionAlreadyClosed {
                id: id.u64().into(),
                close_time: closed.close_time,
                reason: closed.reason.to_string(),
            }
            .into_anyhow()),
            _ => Err(e),
        },
    }?;
    if let Some(slippage_assert) = slippage_assert {
        let market_type = state.market_id(store)?.get_market_type();
        state.do_slippage_assert(
            store,
            slippage_assert,
            -pos.notional_size,
            market_type,
            Some(pos.liquidation_margin.delta_neutrality),
            &price_point,
        )?;
    }
    ClosePositionExec::new_via_msg(state, store, pos, price_point)
}
