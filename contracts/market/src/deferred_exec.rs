use msg::contracts::market::{
    deferred_execution::{
        DeferredExecCompleteTarget, DeferredExecId, DeferredExecItem, DeferredExecWithStatus,
    },
    entry::SlippageAssert,
    position::{events::PositionSaveReason, CollateralAndUsd},
};

use crate::state::position::OpenPositionParams;
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
        let pos_order_id = helper(self, ctx, item.clone(), price_point)?;
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

fn helper(
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
            handle_update_position_shared(state, ctx, id, None, None, &price_point)?;
            state.update_position_collateral(ctx, id, amount.into_signed(), &price_point)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionAddCollateralImpactSize {
            id,
            slippage_assert,
            amount,
        } => {
            let funds = amount.into_signed();
            let notional_size = state.update_size_new_notional_size(ctx.storage, id, funds)?;
            handle_update_position_shared(
                state,
                ctx,
                id,
                Some(notional_size),
                slippage_assert,
                &price_point,
            )?;
            state.update_position_size(ctx, id, funds, &price_point)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { id, amount } => {
            handle_update_position_shared(state, ctx, id, None, None, &price_point)?;
            state.update_position_collateral(ctx, id, -amount.into_signed(), &price_point)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactSize {
            id,
            amount,
            slippage_assert,
        } => {
            let funds = -amount.into_signed();
            let notional_size = state.update_size_new_notional_size(ctx.storage, id, funds)?;
            handle_update_position_shared(
                state,
                ctx,
                id,
                Some(notional_size),
                slippage_assert,
                &price_point,
            )?;
            state.update_position_size(ctx, id, funds, &price_point)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionLeverage {
            id,
            leverage,
            slippage_assert,
        } => {
            handle_update_position_shared(state, ctx, id, None, None, &price_point)?;
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
            state.update_position_leverage(ctx, id, notional_size, &price_point)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionMaxGains { id, max_gains } => {
            handle_update_position_shared(state, ctx, id, None, None, &price_point)?;
            let counter_collateral = state.update_max_gains_new_counter_collateral(
                ctx.storage,
                id,
                max_gains,
                &price_point,
            )?;
            state.update_position_max_gains(ctx, id, counter_collateral, &price_point)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::ClosePosition {
            id,
            slippage_assert,
        } => {
            let pos = get_position(ctx.storage, id)?;
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
            handle_update_position_shared(state, ctx, id, None, None, &price_point)?;
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

fn handle_update_position_shared(
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
        DeferredExecItem::UpdatePositionAddCollateralImpactSize {
            id,
            slippage_assert,
            amount,
        } => {
            let funds = amount.into_signed();
            let notional_size = state.update_size_new_notional_size(store, id, funds)?;
            update_position_slippage_assert(
                state,
                store,
                id,
                Some(notional_size),
                slippage_assert,
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
            update_position_slippage_assert(
                state,
                store,
                id,
                Some(notional_size),
                slippage_assert,
                price_point,
            )?;
            Ok(())
        }
        DeferredExecItem::UpdatePositionLeverage {
            id,
            leverage,
            slippage_assert,
        } => {
            let notional_size =
                state.update_leverage_new_notional_size(store, id, leverage, price_point)?;
            // This slippage assert is not 100% the same as in the execute code path, because
            // it is done before liquifund while the real slippage assert test is done after.
            update_position_slippage_assert(
                state,
                store,
                id,
                Some(notional_size),
                slippage_assert,
                price_point,
            )?;
            Ok(())
        }
        DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { .. }
        | DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { .. }
        | DeferredExecItem::UpdatePositionMaxGains { .. }
        | DeferredExecItem::SetTriggerOrder { .. }
        | DeferredExecItem::PlaceLimitOrder { .. }
        | DeferredExecItem::CancelLimitOrder { .. } => Ok(()),
    }
}
