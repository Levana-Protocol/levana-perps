// CosmWasm 1.0 redacts submessage errors, turning them all into "error code 5"
// This will be changed in Cosmwasm 2.0, but for now, we need to recover errors manually
//
// The validation branch tries to follow the execution branch as closely as possible
// by way of shared functions and structs that contain as much of the validation as possible in their construction
//
// The execution branch then goes one step further - applying the changes to the state, using that same struct
//
// In some cases there are validation steps which are run again at apply() time,
// which re-validate certain conditions (like open-interest) with the true state having changed
use msg::contracts::market::{
    deferred_execution::{
        DeferredExecCompleteTarget, DeferredExecId, DeferredExecItem, DeferredExecWithStatus,
    },
    entry::SlippageAssert,
    position::{events::PositionSaveReason, CollateralAndUsd},
};

use crate::state::position::{
    liquifund::PositionLiquifund,
    update::{
        UpdatePositionCollateral, UpdatePositionLeverage, UpdatePositionMaxGains,
        UpdatePositionSize,
    },
    OpenPositionParams,
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
    ) -> Result<()> {
        let item = self.load_deferred_exec_item(store, id)?;
        helper_validate(self, store, item, price_point)
    }
}

fn helper_execute(
    state: &State,
    ctx: &mut StateContext,
    item: DeferredExecWithStatus,
    price_point: PricePoint,
) -> Result<DeferredExecCompleteTarget> {
    match item.item {
        DeferredExecItem::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
            amount,
            crank_fee,
            crank_fee_usd,
        } => state
            .handle_position_open(
                ctx,
                item.owner,
                amount,
                leverage,
                direction,
                max_gains,
                slippage_assert,
                stop_loss_override,
                take_profit_override,
                crank_fee,
                crank_fee_usd,
                &price_point,
            )
            .map(DeferredExecCompleteTarget::Position),
        DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { id, amount } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            UpdatePositionCollateral::new(
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
            UpdatePositionSize::new(
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
            UpdatePositionCollateral::new(
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
            UpdatePositionSize::new(
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
                let delta_notional_size = notional_size - pos.notional_size;
                state.do_slippage_assert(
                    ctx.storage,
                    slippage_assert,
                    delta_notional_size,
                    market_type,
                    Some(pos.liquidation_margin.delta_neutrality),
                    &price_point,
                )?;
            }
            UpdatePositionLeverage::new(
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
            UpdatePositionMaxGains::new(
                state,
                ctx.storage,
                get_position(ctx.storage, id)?,
                max_gains,
                &price_point,
            )?
            .apply(state, ctx)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::ClosePosition {
            id,
            slippage_assert,
        } => {
            let pos = match get_position(ctx.storage, id) {
                Ok(pos) => Ok(pos),
                Err(e) => match state.load_closed_position(ctx.storage, id) {
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
                let market_type = state.market_id(ctx.storage)?.get_market_type();
                state.do_slippage_assert(
                    ctx.storage,
                    slippage_assert,
                    -pos.notional_size,
                    market_type,
                    Some(pos.liquidation_margin.delta_neutrality),
                    &price_point,
                )?;
            }
            state.close_position_via_msg(ctx, pos, price_point)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::SetTriggerOrder {
            id,
            stop_loss_override,
            take_profit_override,
        } => {
            execute_slippage_assert_and_liquifund(state, ctx, id, None, None, &price_point)?;
            state.set_trigger_order(
                ctx,
                id,
                stop_loss_override,
                take_profit_override,
                &price_point,
            )?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::PlaceLimitOrder {
            trigger_price,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
            amount,
            crank_fee,
            crank_fee_usd,
        } => {
            let market_type = state.market_id(ctx.storage)?.get_market_type();

            let order_id = state.limit_order_set_order(
                ctx,
                item.owner,
                trigger_price,
                amount,
                leverage,
                direction.into_notional(market_type),
                max_gains,
                stop_loss_override,
                take_profit_override,
                crank_fee,
                crank_fee_usd,
                &price_point,
            )?;
            Ok(DeferredExecCompleteTarget::Order(order_id))
        }
        DeferredExecItem::CancelLimitOrder { order_id } => {
            state.limit_order_cancel_order(ctx, order_id)?;
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
) -> Result<()> {
    match item.item {
        DeferredExecItem::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
            amount,
            crank_fee,
            crank_fee_usd,
        } => state
            .validate_new_position(
                store,
                OpenPositionParams {
                    owner: item.owner,
                    collateral: amount,
                    crank_fee: CollateralAndUsd::from_pair(crank_fee, crank_fee_usd),
                    leverage,
                    direction,
                    max_gains_in_quote: max_gains,
                    slippage_assert,
                    stop_loss_override,
                    take_profit_override,
                },
                price_point,
            )
            .map(|_| ()),
        DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { id, amount } => {
            let liquifund =
                validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?;

            let _ = UpdatePositionCollateral::new(
                state,
                store,
                liquifund.position.inner_position().clone(),
                amount.into_signed(),
                price_point,
            )?;
            Ok(())
        }
        DeferredExecItem::UpdatePositionAddCollateralImpactSize {
            id,
            slippage_assert,
            amount,
        } => {
            let funds = amount.into_signed();

            let notional_size = state.update_size_new_notional_size(store, id, funds)?;
            let liquifund = validate_slippage_assert_and_liquifund(
                state,
                store,
                id,
                Some(notional_size),
                slippage_assert,
                price_point,
            )?;
            let _ = UpdatePositionSize::new(
                state,
                store,
                liquifund.position.inner_position().clone(),
                funds,
                price_point,
            )?;

            Ok(())
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { id, amount } => {
            let liquifund =
                validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?;
            let _ = UpdatePositionCollateral::new(
                state,
                store,
                liquifund.position.inner_position().clone(),
                -amount.into_signed(),
                price_point,
            )?;
            Ok(())
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactSize {
            id,
            amount,
            slippage_assert,
        } => {
            let funds = -amount.into_signed();
            let notional_size = state.update_size_new_notional_size(store, id, funds)?;
            let liquifund = validate_slippage_assert_and_liquifund(
                state,
                store,
                id,
                Some(notional_size),
                slippage_assert,
                price_point,
            )?;
            let _ = UpdatePositionSize::new(
                state,
                store,
                liquifund.position.inner_position().clone(),
                funds,
                price_point,
            )?;
            Ok(())
        }
        DeferredExecItem::UpdatePositionLeverage {
            id,
            leverage,
            slippage_assert,
        } => {
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

            let pos = liquifund.position.inner_position().clone();

            if let Some(slippage_assert) = slippage_assert {
                let market_type = state.market_id(store)?.get_market_type();

                let delta_notional_size = notional_size - pos.notional_size;
                state.do_slippage_assert(
                    store,
                    slippage_assert,
                    delta_notional_size,
                    market_type,
                    Some(pos.liquidation_margin.delta_neutrality),
                    price_point,
                )?;
            }

            let _ = UpdatePositionLeverage::new(state, store, pos, notional_size, price_point)?;
            Ok(())
        }
        DeferredExecItem::UpdatePositionMaxGains { id, max_gains } => {
            let liquifund =
                validate_slippage_assert_and_liquifund(state, store, id, None, None, price_point)?;
            let _ = UpdatePositionMaxGains::new(
                state,
                store,
                liquifund.position.inner_position().clone(),
                max_gains,
                price_point,
            )?;
            Ok(())
        }
        DeferredExecItem::ClosePosition {
            id,
            slippage_assert,
        } => {
            let pos = get_position(store, id)?;
            if let Some(slippage_assert) = slippage_assert {
                let market_type = state.market_id(store)?.get_market_type();
                state.do_slippage_assert(
                    store,
                    slippage_assert,
                    -pos.notional_size,
                    market_type,
                    Some(pos.liquidation_margin.delta_neutrality),
                    price_point,
                )?;
            }
            Ok(())
        }
        DeferredExecItem::SetTriggerOrder { .. }
        | DeferredExecItem::PlaceLimitOrder { .. }
        | DeferredExecItem::CancelLimitOrder { .. } => Ok(()),
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
    state.position_liquifund(store, pos, starts_at, price_point.timestamp, false)
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
        let delta_notional_size = notional_size.unwrap_or(pos.notional_size) - pos.notional_size;
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
