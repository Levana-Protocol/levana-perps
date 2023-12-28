use msg::contracts::market::{
    deferred_execution::{
        DeferredExecCompleteTarget, DeferredExecId, DeferredExecItem, DeferredExecWithStatus,
    },
    entry::SlippageAssert,
    position::events::PositionSaveReason,
};

use crate::state::position::OpenPositionParams;
use crate::{prelude::*, state::position::get_position};

impl State<'_> {
    pub(crate) fn perform_deferred_exec(
        &self,
        ctx: &mut StateContext,
        id: DeferredExecId,
    ) -> Result<()> {
        let item = self.load_deferred_exec_item(ctx.storage, id)?;
        let pos_order_id = helper(self, ctx, item.clone())?;
        self.mark_deferred_exec_success(ctx, item, pos_order_id)?;
        Ok(())
    }

    pub(crate) fn deferred_validate(&self, store: &dyn Storage, id: DeferredExecId) -> Result<()> {
        let item = self.load_deferred_exec_item(store, id)?;
        helper_validate(self, store, item)
    }
}

fn helper(
    state: &State,
    ctx: &mut StateContext,
    item: DeferredExecWithStatus,
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
            )
            .map(DeferredExecCompleteTarget::Position),
        DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { id, amount } => {
            handle_update_position_shared(state, ctx, id, None, None)?;
            state.update_position_collateral(ctx, id, amount.into_signed())?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionAddCollateralImpactSize {
            id,
            slippage_assert,
            amount,
        } => {
            let funds = amount.into_signed();
            let notional_size = state.update_size_new_notional_size(ctx, id, funds)?;
            handle_update_position_shared(state, ctx, id, Some(notional_size), slippage_assert)?;
            state.update_position_size(ctx, id, funds)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { id, amount } => {
            handle_update_position_shared(state, ctx, id, None, None)?;
            state.update_position_collateral(ctx, id, -amount.into_signed())?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionRemoveCollateralImpactSize {
            id,
            amount,
            slippage_assert,
        } => {
            let funds = -amount.into_signed();
            let notional_size = state.update_size_new_notional_size(ctx, id, funds)?;
            handle_update_position_shared(state, ctx, id, Some(notional_size), slippage_assert)?;
            state.update_position_size(ctx, id, funds)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionLeverage {
            id,
            leverage,
            slippage_assert,
        } => {
            handle_update_position_shared(state, ctx, id, None, None)?;
            let notional_size = state.update_leverage_new_notional_size(ctx, id, leverage)?;
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
                )?;
            }
            state.update_position_leverage(ctx, id, notional_size)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::UpdatePositionMaxGains { id, max_gains } => {
            handle_update_position_shared(state, ctx, id, None, None)?;
            let counter_collateral =
                state.update_max_gains_new_counter_collateral(ctx, id, max_gains)?;
            state.update_position_max_gains(ctx, id, counter_collateral)?;
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
                )?;
            }
            state.close_position_via_msg(ctx, pos)?;
            Ok(DeferredExecCompleteTarget::Position(id))
        }
        DeferredExecItem::SetTriggerOrder {
            id,
            stop_loss_override,
            take_profit_override,
        } => {
            state.set_trigger_order(ctx, id, stop_loss_override, take_profit_override)?;
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
            )?;
            Ok(DeferredExecCompleteTarget::Order(order_id))
        }
        DeferredExecItem::CancelLimitOrder { order_id } => {
            state.limit_order_cancel_order(ctx, order_id)?;
            Ok(DeferredExecCompleteTarget::Order(order_id))
        }
    }
}

fn handle_update_position_shared(
    state: &State,
    ctx: &mut StateContext,
    id: PositionId,
    notional_size: Option<Signed<Notional>>,
    slippage_assert: Option<SlippageAssert>,
) -> Result<()> {
    // We used to assert position owner here, but that's now handled when queueing the deferred message.

    if let Some(slippage_assert) = slippage_assert {
        let market_type = state.market_id(ctx.storage)?.get_market_type();
        let pos = get_position(ctx.storage, id)?;
        let delta_notional_size = notional_size.unwrap_or(pos.notional_size) - pos.notional_size;
        state.do_slippage_assert(
            ctx.storage,
            slippage_assert,
            delta_notional_size,
            market_type,
            Some(pos.liquidation_margin.delta_neutrality),
        )?;
    }

    let now = state.now();
    let pos = get_position(ctx.storage, id)?;

    let starts_at = pos.liquifunded_at;
    state.position_liquifund_store(ctx, pos, starts_at, now, false, PositionSaveReason::Update)?;

    Ok(())
}

fn helper_validate(state: &State, store: &dyn Storage, item: DeferredExecWithStatus) -> Result<()> {
    match item.item {
        DeferredExecItem::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
            amount,
        } => state
            .validate_new_position(
                store,
                OpenPositionParams {
                    owner: item.owner,
                    collateral: amount,
                    leverage,
                    direction,
                    max_gains_in_quote: max_gains,
                    slippage_assert,
                    stop_loss_override,
                    take_profit_override,
                },
            )
            .map(|_| ()),
        DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { .. }
        | DeferredExecItem::UpdatePositionAddCollateralImpactSize { .. }
        | DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { .. }
        | DeferredExecItem::UpdatePositionRemoveCollateralImpactSize { .. }
        | DeferredExecItem::UpdatePositionLeverage { .. }
        | DeferredExecItem::UpdatePositionMaxGains { .. }
        | DeferredExecItem::ClosePosition { .. }
        | DeferredExecItem::SetTriggerOrder { .. }
        | DeferredExecItem::PlaceLimitOrder { .. }
        | DeferredExecItem::CancelLimitOrder { .. } => Ok(()),
    }
}
