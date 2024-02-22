use crate::prelude::*;
use crate::state::delta_neutrality_fee::ChargeDeltaNeutralityFeeResult;
use crate::state::history::trade::trade_volume_usd;
use crate::state::liquidity::{LiquidityLock, LiquidityUnlock};
use crate::state::position::get_position;
use msg::contracts::market::delta_neutrality_fee::DeltaNeutralityFeeReason;
use msg::contracts::market::entry::PositionActionKind;
use msg::contracts::market::fees::events::FeeSource;
use msg::contracts::market::position::events::{
    calculate_position_collaterals, PositionAttributes, PositionSaveReason, PositionTradingFee,
};
use msg::contracts::market::position::{events::PositionUpdateEvent, Position, PositionId};

use super::AdjustOpenInterest;

impl State<'_> {
    pub(crate) fn update_leverage_new_notional_size(
        &self,
        store: &dyn Storage,
        id: PositionId,
        leverage: LeverageToBase,
        price_point: &PricePoint,
    ) -> Result<Signed<Notional>> {
        let market_type = self.market_id(store)?.get_market_type();
        let pos = get_position(store, id)?;

        let leverage_to_base = leverage.into_signed(pos.direction().into_base(market_type));

        let leverage_to_notional = leverage_to_base.into_notional(market_type);

        let notional_size_in_collateral =
            leverage_to_notional.checked_mul_collateral(pos.active_collateral)?;

        Ok(notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x)))
    }

    pub(crate) fn update_size_new_notional_size(
        &self,
        store: &dyn Storage,
        id: PositionId,
        collateral_delta: Signed<Collateral>,
    ) -> Result<Signed<Notional>> {
        let pos = get_position(store, id)?;
        let scale_factor = (pos.active_collateral.into_number() + collateral_delta.into_number())
            .checked_div(pos.active_collateral.into_number())?;
        Ok(Signed::<Notional>::from_number(
            pos.notional_size.into_number() * scale_factor,
        ))
    }

    pub(crate) fn update_max_gains_new_counter_collateral(
        &self,
        store: &dyn Storage,
        id: PositionId,
        max_gains_in_quote: MaxGainsInQuote,
        price_point: &PricePoint,
    ) -> Result<NonZero<Collateral>> {
        let pos = get_position(store, id)?;
        let market_type = self.market_id(store)?.get_market_type();

        let counter_collateral = match market_type {
            MarketType::CollateralIsQuote => {
                let max_gains_in_collateral = match max_gains_in_quote {
                    MaxGainsInQuote::PosInfinity => {
                        return Err(MarketError::InvalidInfiniteMaxGains {
                            market_type,
                            direction: pos.direction().into_base(market_type),
                        }
                        .into());
                    }
                    MaxGainsInQuote::Finite(x) => x,
                };

                pos.active_collateral
                    .checked_mul_non_zero(max_gains_in_collateral)?
            }
            MarketType::CollateralIsBase => max_gains_in_quote.calculate_counter_collateral(
                market_type,
                pos.active_collateral,
                pos.notional_size
                    .map(|x| price_point.notional_to_collateral(x)),
                pos.active_leverage_to_notional(price_point),
            )?,
        };

        Ok(counter_collateral)
    }

    fn position_update_event(
        &self,
        store: &dyn Storage,
        original_pos: &Position,
        pos: Position,
        spot_price: &PricePoint,
    ) -> Result<PositionUpdateEvent> {
        let new_leverage = pos.active_leverage_to_notional(spot_price);
        let new_counter_leverage = pos.counter_leverage_to_notional(spot_price);
        let old_leverage = original_pos.active_leverage_to_notional(spot_price);
        let old_counter_leverage = original_pos.counter_leverage_to_notional(spot_price);

        let market_id = self.market_id(store)?;
        let market_type = market_id.get_market_type();

        // For PERP-996, ensure that the direction to base didn't flip.
        let old_direction_to_base = old_leverage.into_base(market_type).split().0;
        let new_direction_to_base = new_leverage.into_base(market_type).split().0;
        if old_direction_to_base != new_direction_to_base {
            // Sanity check: max gains should not be computable in this case.
            debug_assert_eq!(
                Err(()),
                pos.max_gains_in_quote(market_type, spot_price)
                    .map_err(|_| ())
            );

            perp_bail!(
                ErrorId::DirectionToBaseFlipped,
                ErrorDomain::Market,
                "Position updates caused the direction to base to flip"
            )
        }

        let leverage_delta = new_leverage.into_number() - old_leverage.into_number();
        let counter_collateral_delta =
            pos.counter_collateral.into_signed() - original_pos.counter_collateral.into_signed();
        let counter_leverage_delta =
            new_counter_leverage.into_number() - old_counter_leverage.into_number();
        let active_collateral_delta =
            pos.active_collateral.into_signed() - original_pos.active_collateral.into_signed();
        let trading_fee_delta =
            pos.trading_fee.collateral() - original_pos.trading_fee.collateral();
        let delta_neutrality_fee_delta =
            pos.delta_neutrality_fee.collateral() - original_pos.delta_neutrality_fee.collateral();
        let notional_size_delta = pos.notional_size - original_pos.notional_size;
        let notional_size_abs_delta = pos.notional_size.abs() - original_pos.notional_size.abs();

        let deposit_collateral_delta =
            pos.deposit_collateral.collateral() - original_pos.deposit_collateral.collateral();
        let collaterals = calculate_position_collaterals(&pos)?;
        let trading_fee = PositionTradingFee {
            trading_fee: pos.trading_fee.collateral(),
            trading_fee_usd: pos.trading_fee.usd(),
        };

        let (direction, leverage) = new_leverage.into_base(market_type).split();
        let (_, counter_leverage) = new_counter_leverage.into_base(market_type).split();
        let notional_size = pos.notional_size;
        let evt = PositionUpdateEvent {
            position_attributes: PositionAttributes {
                pos_id: pos.id,
                owner: pos.owner.clone(),
                collaterals,
                trading_fee,
                market_type,
                notional_size,
                notional_size_in_collateral: notional_size
                    .map(|notional_size| spot_price.notional_to_collateral(notional_size)),
                notional_size_usd: notional_size
                    .map(|notional_size| spot_price.notional_to_usd(notional_size)),
                direction,
                leverage,
                counter_leverage,
                stop_loss_override: pos.stop_loss_override,
                take_profit_override: pos.take_profit_override,
            },
            deposit_collateral_delta,
            deposit_collateral_delta_usd: deposit_collateral_delta
                .map(|x| spot_price.collateral_to_usd(x)),
            active_collateral_delta,
            active_collateral_delta_usd: active_collateral_delta
                .map(|x| spot_price.collateral_to_usd(x)),
            counter_collateral_delta,
            counter_collateral_delta_usd: counter_collateral_delta
                .map(|x| spot_price.collateral_to_usd(x)),
            leverage_delta,
            counter_leverage_delta,
            notional_size_delta,
            notional_size_delta_usd: notional_size_delta.map(|x| spot_price.notional_to_usd(x)),
            notional_size_abs_delta,
            notional_size_abs_delta_usd: notional_size_abs_delta
                .map(|x| spot_price.notional_to_usd(x)),
            trading_fee_delta,
            trading_fee_delta_usd: spot_price.collateral_to_usd(trading_fee_delta),
            delta_neutrality_fee_delta,
            delta_neutrality_fee_delta_usd: delta_neutrality_fee_delta
                .map(|x| spot_price.collateral_to_usd(x)),
            updated_at: self.now(),
        };

        Ok(evt)
    }
}

// This is a helper struct, created from read-only storage.
// Most of the validation is done while creating the struct, which allows for
// error recovery in the submessage handler.
// State is then mutably updated by calling .apply()
#[must_use]
pub(crate) struct UpdatePositionCollateralExec {
    pos: Position,
    price_point: PricePoint,
    trade_volume: Usd,
    user_refund: Option<NonZero<Collateral>>,
    event: PositionUpdateEvent,
}
impl UpdatePositionCollateralExec {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        pos: Position,
        collateral_delta: Signed<Collateral>,
        price_point: &PricePoint,
    ) -> Result<Self> {
        let mut pos = pos;
        let original_pos = pos.clone();

        // Update
        pos.active_collateral = pos.active_collateral.checked_add_signed(collateral_delta)?;
        pos.deposit_collateral
            .checked_add_assign(collateral_delta, price_point)?;

        let market_type = state.market_id(store)?.get_market_type();

        let trade_volume = trade_volume_usd(&original_pos, price_point, market_type)?
            .diff(trade_volume_usd(&pos, price_point, market_type)?);

        // Validate
        state.position_validate_leverage_data(
            market_type,
            &pos,
            price_point,
            Some(&original_pos),
        )?;
        if collateral_delta.is_negative() {
            state.validate_minimum_deposit_collateral(pos.active_collateral.raw(), price_point)?;
        }

        // calculate possible user refund
        let user_refund = (-collateral_delta).try_into_non_zero();

        let event = state.position_update_event(store, &original_pos, pos.clone(), price_point)?;

        Ok(UpdatePositionCollateralExec {
            pos,
            price_point: *price_point,
            trade_volume,
            user_refund,
            event,
        })
    }

    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}
    pub(crate) fn apply(mut self, state: &State, ctx: &mut StateContext) -> Result<()> {
        debug_assert!(self.pos.liquifunded_at == self.price_point.timestamp);

        state.trade_history_add_volume(ctx, &self.pos.owner, self.trade_volume)?;

        self.event.emit(state, ctx, &self.pos, &self.price_point)?;

        state.position_save(
            ctx,
            &mut self.pos,
            &self.price_point,
            true,
            true,
            PositionSaveReason::Update,
        )?;

        // Refund if needed
        if let Some(user_refund) = self.user_refund {
            // send extracted collateral back to the user
            state.add_token_transfer_msg(ctx, &self.pos.owner, user_refund)?;
        }
        Ok(())
    }
}

// This is a helper struct, created from read-only storage.
// Most of the validation is done while creating the struct, which allows for
// error recovery in the submessage handler.
// State is then mutably updated by calling .apply()
#[must_use]
pub(crate) struct UpdatePositionSizeExec {
    pos: Position,
    price_point: PricePoint,
    trade_volume: Usd,
    user_refund: Option<NonZero<Collateral>>,
    dnf: ChargeDeltaNeutralityFeeResult,
    open_interest: AdjustOpenInterest,
    liquidity_update: Option<LiquidityUpdate>,
    trading_fee_delta: Collateral,
    event: PositionUpdateEvent,
}

impl UpdatePositionSizeExec {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        pos: Position,
        collateral_delta: Signed<Collateral>,
        price_point: &PricePoint,
    ) -> Result<Self> {
        let mut pos = pos;
        let original_pos = pos.clone();

        // Update
        if !pos
            .active_collateral
            .into_signed()
            .checked_add(collateral_delta)?
            .into_number()
            .approx_gt_strict(Number::ZERO)
        {
            perp_bail!(
                ErrorId::PositionUpdate,
                ErrorDomain::Market,
                "Active collateral cannot be negative!"
            );
        }

        let scale_factor = ((pos.active_collateral.into_number() + collateral_delta.into_number())
            / pos.active_collateral.into_number())
        .try_into_non_zero()
        .context("scale_factor is negative or zero")?;
        pos.active_collateral = pos.active_collateral.checked_add_signed(collateral_delta)?;
        pos.deposit_collateral
            .checked_add_assign(collateral_delta, price_point)?;
        let old_notional_size_in_collateral = pos.notional_size_in_collateral(price_point);
        let new_notional_size_in_collateral =
            old_notional_size_in_collateral.try_map(|x| x.checked_mul_dec(scale_factor.raw()))?;
        pos.notional_size =
            new_notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x));

        let old_counter_collateral = pos.counter_collateral.raw();
        let new_counter_collateral = pos.counter_collateral.checked_mul_non_zero(scale_factor)?;
        let counter_collateral_delta =
            new_counter_collateral.into_signed() - old_counter_collateral.into_signed();
        pos.counter_collateral = new_counter_collateral;

        let market_type = state.market_type(store)?;

        let trade_volume = trade_volume_usd(&original_pos, price_point, market_type)?
            .diff(trade_volume_usd(&pos, price_point, market_type)?);

        // Validate leverage _before_ we reduce trading fees from active collateral
        state.position_validate_leverage_data(
            market_type,
            &pos,
            price_point,
            Some(&original_pos),
        )?;

        if collateral_delta.is_negative() {
            state.validate_minimum_deposit_collateral(pos.active_collateral.raw(), price_point)?;
        }

        // Update fees
        let trading_fee_delta = state.config.calculate_trade_fee(
            old_notional_size_in_collateral,
            new_notional_size_in_collateral,
            old_counter_collateral,
            new_counter_collateral.raw(),
        )?;
        pos.active_collateral = pos.active_collateral.checked_sub(trading_fee_delta)?;
        pos.trading_fee
            .checked_add_assign(trading_fee_delta, price_point)?;

        let notional_size_diff = pos.notional_size - original_pos.notional_size;

        let dnf = state.charge_delta_neutrality_fee(
            store,
            &mut pos,
            notional_size_diff,
            price_point,
            DeltaNeutralityFeeReason::PositionUpdate,
        )?;

        let open_interest =
            AdjustOpenInterest::new(state, store, notional_size_diff, pos.direction(), true)?;

        let liquidity_update = LiquidityUpdate::new(
            state,
            store,
            counter_collateral_delta,
            price_point,
            Some(&open_interest),
        )?;
        // Send the removed collateral back to the user. We convert a negative
        // delta (indicating the user requested collateral be returned) into a
        // positive (giving the amount to be returned) and then attempt to
        // convert to a NonZero to ensure this is a positive value greater than
        // 0. If it's 0 or negative, there's no transfer to be made.
        let user_refund = (-collateral_delta).try_into_non_zero();

        let event = state.position_update_event(store, &original_pos, pos.clone(), price_point)?;

        Ok(UpdatePositionSizeExec {
            pos,
            price_point: *price_point,
            trade_volume,
            user_refund,
            dnf,
            open_interest,
            liquidity_update,
            trading_fee_delta,
            event,
        })
    }
    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}

    pub(crate) fn apply(mut self, state: &State, ctx: &mut StateContext) -> Result<()> {
        debug_assert!(self.pos.liquifunded_at == self.price_point.timestamp);

        state.trade_history_add_volume(ctx, &self.pos.owner, self.trade_volume)?;

        self.dnf.apply(state, ctx)?;
        self.open_interest.apply(ctx)?;

        state.position_save(
            ctx,
            &mut self.pos,
            &self.price_point,
            true,
            true,
            PositionSaveReason::Update,
        )?;

        state.add_delta_neutrality_ratio_event(
            ctx,
            &state.load_liquidity_stats(ctx.storage)?,
            &self.price_point,
        )?;

        if let Some(liquidity_update) = self.liquidity_update.take() {
            liquidity_update.apply(state, ctx)?;
        }
        state.collect_trading_fee(
            ctx,
            self.pos.id,
            self.trading_fee_delta,
            self.price_point,
            FeeSource::Trading,
        )?;

        self.event.emit(state, ctx, &self.pos, &self.price_point)?;

        if let Some(user_refund) = self.user_refund {
            state.add_token_transfer_msg(ctx, &self.pos.owner, user_refund)?;
        }
        Ok(())
    }
}

// This is a helper struct, created from read-only storage.
// Most of the validation is done while creating the struct, which allows for
// error recovery in the submessage handler.
// State is then mutably updated by calling .apply()
#[must_use]
pub(crate) struct UpdatePositionLeverageExec {
    pos: Position,
    price_point: PricePoint,
    trade_volume: Usd,
    dnf: ChargeDeltaNeutralityFeeResult,
    open_interest: AdjustOpenInterest,
    liquidity_update: Option<LiquidityUpdate>,
    trading_fee_delta: Collateral,
    event: PositionUpdateEvent,
}

impl UpdatePositionLeverageExec {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        pos: Position,
        notional_size: Signed<Notional>,
        price_point: &PricePoint,
    ) -> Result<Self> {
        let mut pos = pos;
        let original_pos = pos.clone();

        if notional_size.into_number().approx_eq(Number::ZERO) {
            perp_bail!(
                ErrorId::PositionUpdate,
                ErrorDomain::Market,
                "Notional size cannot be zero!"
            );
        }

        // Update

        let old_counter_ratio_of_notional_size_in_collateral =
            pos.counter_collateral.checked_div_collateral(
                NonZero::new(price_point.notional_to_collateral(pos.notional_size.abs_unsigned()))
                    .context("notional_size is zero")?,
            )?;

        let old_notional_size_in_collateral = pos
            .notional_size
            .map(|x| price_point.notional_to_collateral(x));
        let new_notional_size_in_collateral =
            notional_size.map(|x| price_point.notional_to_collateral(x));
        pos.notional_size = notional_size;

        let old_counter_collateral = pos.counter_collateral;
        let new_counter_collateral = NonZero::new(
            new_notional_size_in_collateral
                .abs_unsigned()
                .checked_mul_dec(old_counter_ratio_of_notional_size_in_collateral.raw())?,
        )
        .context("new_counter_collateral is zero")?;
        let counter_collateral_delta =
            new_counter_collateral.into_signed() - old_counter_collateral.into_signed();
        pos.counter_collateral = new_counter_collateral;

        let market_type = state.market_id(store)?.get_market_type();

        let trade_volume = trade_volume_usd(&original_pos, price_point, market_type)?
            .diff(trade_volume_usd(&pos, price_point, market_type)?);

        // Validate leverage _before_ we reduce trading fees from active collateral
        state.position_validate_leverage_data(
            market_type,
            &pos,
            price_point,
            Some(&original_pos),
        )?;

        // Update fees

        let trading_fee_delta = state.config.calculate_trade_fee(
            old_notional_size_in_collateral,
            new_notional_size_in_collateral,
            old_counter_collateral.raw(),
            new_counter_collateral.raw(),
        )?;

        pos.active_collateral = pos.active_collateral.checked_sub(trading_fee_delta)?;
        pos.trading_fee
            .checked_add_assign(trading_fee_delta, price_point)?;

        // Validation

        let notional_size_diff = pos.notional_size - original_pos.notional_size;

        let dnf = state.charge_delta_neutrality_fee(
            store,
            &mut pos,
            notional_size_diff,
            price_point,
            DeltaNeutralityFeeReason::PositionUpdate,
        )?;

        let open_interest =
            AdjustOpenInterest::new(state, store, notional_size_diff, pos.direction(), true)?;

        let liquidity_update = LiquidityUpdate::new(
            state,
            store,
            counter_collateral_delta,
            price_point,
            Some(&open_interest),
        )?;
        let event = state.position_update_event(store, &original_pos, pos.clone(), price_point)?;

        Ok(UpdatePositionLeverageExec {
            pos,
            price_point: *price_point,
            trade_volume,
            dnf,
            open_interest,
            liquidity_update,
            trading_fee_delta,
            event,
        })
    }

    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}

    pub(crate) fn apply(mut self, state: &State, ctx: &mut StateContext) -> Result<()> {
        debug_assert!(self.pos.liquifunded_at == self.price_point.timestamp);

        state.trade_history_add_volume(ctx, &self.pos.owner, self.trade_volume)?;

        self.dnf.apply(state, ctx)?;
        self.open_interest.apply(ctx)?;

        state.position_save(
            ctx,
            &mut self.pos,
            &self.price_point,
            true,
            true,
            PositionSaveReason::Update,
        )?;

        state.add_delta_neutrality_ratio_event(
            ctx,
            &state.load_liquidity_stats(ctx.storage)?,
            &self.price_point,
        )?;

        if let Some(liquidity_update) = self.liquidity_update.take() {
            liquidity_update.apply(state, ctx)?;
        }

        state.collect_trading_fee(
            ctx,
            self.pos.id,
            self.trading_fee_delta,
            self.price_point,
            FeeSource::Trading,
        )?;
        self.event.emit(state, ctx, &self.pos, &self.price_point)?;
        Ok(())
    }
}

// This is a helper struct, created from read-only storage.
// Most of the validation is done while creating the struct, which allows for
// error recovery in the submessage handler.
// State is then mutably updated by calling .apply()
#[must_use]
pub(crate) struct UpdatePositionMaxGainsExec {
    pos: Position,
    price_point: PricePoint,
    liquidity_update: Option<LiquidityUpdate>,
    trading_fee_delta: Collateral,
    event: PositionUpdateEvent,
}

impl UpdatePositionMaxGainsExec {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        pos: Position,
        max_gains: MaxGainsInQuote,
        price_point: &PricePoint,
    ) -> Result<Self> {
        let mut pos = pos;
        let original_pos = pos.clone();

        let counter_collateral =
            state.update_max_gains_new_counter_collateral(store, pos.id, max_gains, price_point)?;

        let old_counter_collateral = pos.counter_collateral;
        let new_counter_collateral = counter_collateral;
        let counter_collateral_delta =
            new_counter_collateral.into_signed() - old_counter_collateral.into_signed();
        pos.counter_collateral = new_counter_collateral;

        // Validate leverage _before_ we reduce trading fees from active collateral
        state.position_validate_leverage_data(
            state.market_type(store)?,
            &pos,
            price_point,
            Some(&original_pos),
        )?;

        // Update fees.
        let trading_fee_delta = state.config.calculate_trade_fee(
            Signed::zero(),
            Signed::zero(),
            old_counter_collateral.raw(),
            new_counter_collateral.raw(),
        )?;

        pos.active_collateral = pos.active_collateral.checked_sub(trading_fee_delta)?;
        pos.trading_fee
            .checked_add_assign(trading_fee_delta, price_point)?;

        let notional_size_diff = pos.notional_size - original_pos.notional_size;
        debug_assert!(notional_size_diff.is_zero());

        let liquidity_update =
            LiquidityUpdate::new(state, store, counter_collateral_delta, price_point, None)?;

        let event = state.position_update_event(store, &original_pos, pos.clone(), price_point)?;

        Ok(UpdatePositionMaxGainsExec {
            pos,
            price_point: *price_point,
            liquidity_update,
            trading_fee_delta,
            event,
        })
    }

    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}
    pub(crate) fn apply(mut self, state: &State, ctx: &mut StateContext) -> Result<()> {
        debug_assert!(self.pos.liquifunded_at == self.price_point.timestamp);

        state.position_save(
            ctx,
            &mut self.pos,
            &self.price_point,
            true,
            true,
            PositionSaveReason::Update,
        )?;

        if let Some(liquidity_update) = self.liquidity_update.take() {
            liquidity_update.apply(state, ctx)?;
        }
        state.collect_trading_fee(
            ctx,
            self.pos.id,
            self.trading_fee_delta,
            self.price_point,
            FeeSource::Trading,
        )?;
        self.event.emit(state, ctx, &self.pos, &self.price_point)?;
        Ok(())
    }
}

// This trait allows for separating the creation of the event vs. emitting it
// Most of the validation is done while creating the event itself, which allows for
// error recovery in the submessage handler.
// The event is then emitted by calling .emit()
trait PositionUpdateEventExt {
    fn emit(
        &self,
        state: &State,
        ctx: &mut StateContext,
        pos: &Position,
        spot_price: &PricePoint,
    ) -> Result<()>;
}

impl PositionUpdateEventExt for PositionUpdateEvent {
    fn emit(
        &self,
        state: &State,
        ctx: &mut StateContext,
        pos: &Position,
        spot_price: &PricePoint,
    ) -> Result<()> {
        ctx.response_mut().add_event(self.clone());

        state.position_history_add_open_update_action(
            ctx,
            pos,
            PositionActionKind::Update,
            if self.trading_fee_delta.is_zero() {
                None
            } else {
                Some(self.trading_fee_delta)
            },
            if self.delta_neutrality_fee_delta.is_zero() {
                None
            } else {
                Some(self.delta_neutrality_fee_delta)
            },
            self.deposit_collateral_delta,
            *spot_price,
        )?;

        Ok(())
    }
}

#[must_use]
enum LiquidityUpdate {
    Lock(LiquidityLock),
    Unlock(LiquidityUnlock),
}

impl LiquidityUpdate {
    fn new(
        state: &State,
        store: &dyn Storage,
        counter_collateral_delta: Signed<Collateral>,
        price_point: &PricePoint,
        open_interest: Option<&AdjustOpenInterest>,
    ) -> Result<Option<Self>> {
        match NonZero::new(counter_collateral_delta.abs_unsigned()) {
            Some(delta_abs) => {
                if counter_collateral_delta.is_strictly_positive() {
                    let liquidity = LiquidityLock::new(
                        state,
                        store,
                        delta_abs,
                        *price_point,
                        None,
                        open_interest
                            .map(|x| x.net_notional(state, store))
                            .transpose()?,
                        None,
                    )?;
                    Ok(Some(Self::Lock(liquidity)))
                } else {
                    let liquidity =
                        LiquidityUnlock::new(state, store, delta_abs, *price_point, None)?;
                    Ok(Some(Self::Unlock(liquidity)))
                }
            }
            None => {
                debug_assert_eq!(counter_collateral_delta, Signed::zero());
                Ok(None)
            }
        }
    }

    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<()> {
        match self {
            Self::Lock(liquidity) => liquidity.apply(state, ctx),
            Self::Unlock(liquidity) => liquidity.apply(state, ctx),
        }
    }
}

#[must_use]
pub(crate) struct TriggerOrderExec {
    pos: Position,
    price_point: PricePoint,
}

impl TriggerOrderExec {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        id: PositionId,
        stop_loss_override: Option<PriceBaseInQuote>,
        // TODO - make this TakeProfitPrice
        take_profit_override: Option<PriceBaseInQuote>,
        price_point: PricePoint,
    ) -> Result<Self> {
        let mut pos = get_position(store, id)?;
        let market_type = state.market_id(store)?.get_market_type();

        // We've decided _not_ to validate prices here. If the user wants to set a stop loss
        // or take profit that conflicts with the current price, liquidation price, or max
        // gains price, we allow it to proceed, and the override may become relevant again
        // in the future as position characteristics change.
        //
        // OLD CODE:
        //
        // self.position_validate_trigger_orders(&pos, market_type, price_point)?;

        debug_assert!(pos.liquifunded_at == price_point.timestamp);

        pos.stop_loss_override = stop_loss_override;
        pos.stop_loss_override_notional =
            stop_loss_override.map(|x| x.into_notional_price(market_type));
        pos.take_profit_override =
            take_profit_override.map(|x| TakeProfitPrice::Finite(x.into_non_zero()));
        pos.take_profit_override_notional =
            take_profit_override.map(|x| x.into_notional_price(market_type));

        Ok(Self { pos, price_point })
    }

    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}

    pub(crate) fn apply(mut self, state: &State, ctx: &mut StateContext) -> Result<()> {
        state.position_save(
            ctx,
            &mut self.pos,
            &self.price_point,
            true,
            false,
            PositionSaveReason::Update,
        )?;

        Ok(())
    }
}
