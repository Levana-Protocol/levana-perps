use crate::prelude::*;
use crate::state::history::trade::trade_volume_usd;
use crate::state::position::get_position;
use msg::contracts::market::delta_neutrality_fee::DeltaNeutralityFeeReason;
use msg::contracts::market::entry::PositionActionKind;
use msg::contracts::market::fees::events::FeeSource;
use msg::contracts::market::position::events::{
    calculate_position_collaterals, PositionAttributes, PositionTradingFee,
};
use msg::contracts::market::position::MaybeClosedPosition;
use msg::contracts::market::position::{events::PositionUpdateEvent, Position, PositionId};

impl State<'_> {
    pub(crate) fn update_leverage_new_notional_size(
        &self,
        ctx: &mut StateContext,
        id: PositionId,
        leverage: LeverageToBase,
    ) -> Result<Signed<Notional>> {
        let market_type = self.market_id(ctx.storage)?.get_market_type();
        let price_point = self.spot_price(ctx.storage, None)?;
        let pos = get_position(ctx.storage, id)?;

        let leverage_to_base = leverage.into_signed(pos.direction().into_base(market_type));

        let leverage_to_notional = leverage_to_base.into_notional(market_type);

        let notional_size_in_collateral =
            leverage_to_notional.checked_mul_collateral(pos.active_collateral)?;

        Ok(notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x)))
    }

    pub(crate) fn update_size_new_notional_size(
        &self,
        ctx: &mut StateContext,
        id: PositionId,
        collateral_delta: Signed<Collateral>,
    ) -> Result<Signed<Notional>> {
        let pos = get_position(ctx.storage, id)?;
        let scale_factor = (pos.active_collateral.into_number() + collateral_delta.into_number())
            .checked_div(pos.active_collateral.into_number())?;
        Ok(Signed::<Notional>::from_number(
            pos.notional_size.into_number() * scale_factor,
        ))
    }

    pub(crate) fn update_max_gains_new_counter_collateral(
        &self,
        ctx: &mut StateContext,
        id: PositionId,
        max_gains_in_quote: MaxGainsInQuote,
    ) -> Result<NonZero<Collateral>> {
        let pos = get_position(ctx.storage, id)?;
        let spot_price = self.spot_price(ctx.storage, None)?;
        let market_type = self.market_id(ctx.storage)?.get_market_type();

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
                    .map(|x| spot_price.notional_to_collateral(x)),
                pos.active_leverage_to_notional(&spot_price),
            )?,
        };

        Ok(counter_collateral)
    }

    fn adjust_counter_collateral_locked(
        &self,
        ctx: &mut StateContext,
        counter_collateral_delta: Signed<Collateral>,
    ) -> Result<()> {
        match NonZero::new(counter_collateral_delta.abs_unsigned()) {
            Some(delta_abs) => {
                if counter_collateral_delta.is_strictly_positive() {
                    self.liquidity_lock(ctx, delta_abs)
                } else {
                    self.liquidity_unlock(ctx, delta_abs)
                }
            }
            None => {
                debug_assert_eq!(counter_collateral_delta, Signed::zero());
                Ok(())
            }
        }
    }

    fn position_update_emit_event(
        &self,
        ctx: &mut StateContext,
        original_pos: &Position,
        pos: Position,
        spot_price: PricePoint,
    ) -> Result<()> {
        let new_leverage = pos.active_leverage_to_notional(&spot_price);
        let new_counter_leverage = pos.counter_leverage_to_notional(&spot_price);
        let old_leverage = original_pos.active_leverage_to_notional(&spot_price);
        let old_counter_leverage = original_pos.counter_leverage_to_notional(&spot_price);

        let market_id = self.market_id(ctx.storage)?;
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

        ctx.response_mut().add_event(evt);
        self.position_history_add_action(
            ctx,
            &pos,
            PositionActionKind::Update,
            if trading_fee_delta.is_zero() {
                None
            } else {
                Some(trading_fee_delta)
            },
            if delta_neutrality_fee_delta.is_zero() {
                None
            } else {
                Some(delta_neutrality_fee_delta)
            },
            spot_price,
        )?;

        Ok(())
    }

    pub fn update_position_collateral(
        &self,
        ctx: &mut StateContext,
        id: PositionId,
        collateral_delta: Signed<Collateral>,
    ) -> Result<()> {
        let spot_price = self.spot_price(ctx.storage, None)?;
        let mut pos = get_position(ctx.storage, id)?;

        let original_pos = pos.clone();

        // Update
        pos.active_collateral = pos.active_collateral.checked_add_signed(collateral_delta)?;
        pos.deposit_collateral
            .checked_add_assign(collateral_delta, &spot_price)?;

        let market_type = self.market_id(ctx.storage)?.get_market_type();

        self.trade_history_add_volume(
            ctx,
            &pos.owner,
            trade_volume_usd(&original_pos, spot_price, market_type)?.diff(trade_volume_usd(
                &pos,
                spot_price,
                market_type,
            )?),
        )?;

        // Storage and external
        self.position_update_emit_event(ctx, &original_pos, pos.clone(), spot_price)?;
        self.position_save(ctx, &mut pos, &spot_price, true, true)?;

        // Validate
        self.position_validate_leverage_data(market_type, &pos, &spot_price, Some(&original_pos))?;
        if collateral_delta.is_negative() {
            self.validate_minimum_deposit_collateral(ctx.storage, pos.active_collateral.raw())?;
        }

        // Refund if needed
        let user_refund = -collateral_delta;
        if let Some(user_refund) = user_refund.try_into_non_zero() {
            // send extracted collateral back to the user
            self.add_token_transfer_msg(ctx, &pos.owner, user_refund)?;
        }

        Ok(())
    }

    pub fn update_position_leverage(
        &self,
        ctx: &mut StateContext,
        id: PositionId,
        notional_size: Signed<Notional>,
    ) -> Result<()> {
        let spot_price = self.spot_price(ctx.storage, None)?;
        let mut pos = get_position(ctx.storage, id)?;

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
                NonZero::new(spot_price.notional_to_collateral(pos.notional_size.abs_unsigned()))
                    .context("notional_size is zero")?,
            )?;

        let old_notional_size_in_collateral = pos
            .notional_size
            .map(|x| spot_price.notional_to_collateral(x));
        let new_notional_size_in_collateral =
            notional_size.map(|x| spot_price.notional_to_collateral(x));
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

        let market_type = self.market_id(ctx.storage)?.get_market_type();

        self.trade_history_add_volume(
            ctx,
            &pos.owner,
            trade_volume_usd(&original_pos, spot_price, market_type)?.diff(trade_volume_usd(
                &pos,
                spot_price,
                market_type,
            )?),
        )?;

        // Validate leverage _before_ we reduce trading fees from active collateral
        self.position_validate_leverage_data(market_type, &pos, &spot_price, Some(&original_pos))?;

        // Update fees

        let trading_fee_delta = self.config.calculate_trade_fee(
            old_notional_size_in_collateral,
            new_notional_size_in_collateral,
            old_counter_collateral.raw(),
            new_counter_collateral.raw(),
        )?;

        pos.active_collateral = pos.active_collateral.checked_sub(trading_fee_delta)?;
        pos.trading_fee
            .checked_add_assign(trading_fee_delta, &spot_price)?;

        // Validation

        let notional_size_diff = pos.notional_size - original_pos.notional_size;

        // Storage and external

        self.charge_delta_neutrality_fee(
            ctx.storage,
            &mut pos,
            notional_size_diff,
            spot_price,
            DeltaNeutralityFeeReason::PositionUpdate,
        )?
        .store(self, ctx)?;
        self.position_save(ctx, &mut pos, &spot_price, true, true)?;
        self.adjust_net_open_interest(ctx, notional_size_diff, pos.direction(), true)?;
        self.add_delta_neutrality_ratio_event(
            ctx,
            &self.load_liquidity_stats(ctx.storage)?,
            &spot_price,
        )?;

        let funding_timestamp = self.funding_valid_until(ctx.storage)?;
        self.accumulate_funding_rate(ctx, funding_timestamp)?;
        self.adjust_counter_collateral_locked(ctx, counter_collateral_delta)?;
        self.collect_trading_fee(
            ctx,
            pos.id,
            trading_fee_delta,
            spot_price,
            FeeSource::Trading,
        )?;
        self.position_update_emit_event(ctx, &original_pos, pos.clone(), spot_price)?;

        Ok(())
    }

    pub fn update_position_size(
        &self,
        ctx: &mut StateContext,
        id: PositionId,
        collateral_delta: Signed<Collateral>,
    ) -> Result<()> {
        let spot_price = self.spot_price(ctx.storage, None)?;
        let mut pos = get_position(ctx.storage, id)?;

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
            .checked_add_assign(collateral_delta, &spot_price)?;
        let old_notional_size_in_collateral = pos.notional_size_in_collateral(&spot_price);
        let new_notional_size_in_collateral =
            old_notional_size_in_collateral.try_map(|x| x.checked_mul_dec(scale_factor.raw()))?;
        pos.notional_size =
            new_notional_size_in_collateral.map(|x| spot_price.collateral_to_notional(x));

        let old_counter_collateral = pos.counter_collateral.raw();
        let new_counter_collateral = pos.counter_collateral.checked_mul_non_zero(scale_factor)?;
        let counter_collateral_delta =
            new_counter_collateral.into_signed() - old_counter_collateral.into_signed();
        pos.counter_collateral = new_counter_collateral;

        let market_type = self.market_type(ctx.storage)?;

        self.trade_history_add_volume(
            ctx,
            &pos.owner,
            trade_volume_usd(&original_pos, spot_price, market_type)?.diff(trade_volume_usd(
                &pos,
                spot_price,
                market_type,
            )?),
        )?;

        // Validate leverage _before_ we reduce trading fees from active collateral
        self.position_validate_leverage_data(market_type, &pos, &spot_price, Some(&original_pos))?;

        if collateral_delta.is_negative() {
            self.validate_minimum_deposit_collateral(ctx.storage, pos.active_collateral.raw())?;
        }

        // Update fees
        let trading_fee_delta = self.config.calculate_trade_fee(
            old_notional_size_in_collateral,
            new_notional_size_in_collateral,
            old_counter_collateral,
            new_counter_collateral.raw(),
        )?;
        pos.active_collateral = pos.active_collateral.checked_sub(trading_fee_delta)?;
        pos.trading_fee
            .checked_add_assign(trading_fee_delta, &spot_price)?;

        let notional_size_diff = pos.notional_size - original_pos.notional_size;

        // Storage and external

        self.charge_delta_neutrality_fee(
            ctx.storage,
            &mut pos,
            notional_size_diff,
            spot_price,
            DeltaNeutralityFeeReason::PositionUpdate,
        )?
        .store(self, ctx)?;
        self.position_save(ctx, &mut pos, &spot_price, true, true)?;
        self.adjust_net_open_interest(ctx, notional_size_diff, pos.direction(), true)?;
        self.add_delta_neutrality_ratio_event(
            ctx,
            &self.load_liquidity_stats(ctx.storage)?,
            &spot_price,
        )?;

        let funding_timestamp = self.funding_valid_until(ctx.storage)?;
        self.accumulate_funding_rate(ctx, funding_timestamp)?;
        self.adjust_counter_collateral_locked(ctx, counter_collateral_delta)?;
        self.collect_trading_fee(
            ctx,
            pos.id,
            trading_fee_delta,
            spot_price,
            FeeSource::Trading,
        )?;
        self.position_update_emit_event(ctx, &original_pos, pos.clone(), spot_price)?;

        // Send the removed collateral back to the user. We convert a negative
        // delta (indicating the user requested collateral be returned) into a
        // positive (giving the amount to be returned) and then attempt to
        // convert to a NonZero to ensure this is a positive value greater than
        // 0. If it's 0 or negative, there's no transfer to be made.
        if let Some(collateral_delta) = (-collateral_delta).try_into_non_zero() {
            self.add_token_transfer_msg(ctx, &pos.owner, collateral_delta)?;
        }

        Ok(())
    }

    pub fn update_position_max_gains(
        &self,
        ctx: &mut StateContext,
        id: PositionId,
        counter_collateral: NonZero<Collateral>,
    ) -> Result<()> {
        let spot_price = self.spot_price(ctx.storage, None)?;
        let mut pos = get_position(ctx.storage, id)?;

        let original_pos = pos.clone();

        let old_counter_collateral = pos.counter_collateral;
        let new_counter_collateral = counter_collateral;
        let counter_collateral_delta =
            new_counter_collateral.into_signed() - old_counter_collateral.into_signed();
        pos.counter_collateral = new_counter_collateral;

        // Validate leverage _before_ we reduce trading fees from active collateral
        self.position_validate_leverage_data(
            self.market_type(ctx.storage)?,
            &pos,
            &spot_price,
            Some(&original_pos),
        )?;

        // Update fees.
        let trading_fee_delta = self.config.calculate_trade_fee(
            Signed::zero(),
            Signed::zero(),
            old_counter_collateral.raw(),
            new_counter_collateral.raw(),
        )?;

        pos.active_collateral = pos.active_collateral.checked_sub(trading_fee_delta)?;
        pos.trading_fee
            .checked_add_assign(trading_fee_delta, &spot_price)?;

        self.position_save(ctx, &mut pos, &spot_price, true, true)?;

        let notional_size_diff = pos.notional_size - original_pos.notional_size;
        debug_assert!(notional_size_diff.is_zero());

        // Storage and external

        self.adjust_counter_collateral_locked(ctx, counter_collateral_delta)?;
        self.collect_trading_fee(
            ctx,
            pos.id,
            trading_fee_delta,
            spot_price,
            FeeSource::Trading,
        )?;
        self.position_update_emit_event(ctx, &original_pos, pos.clone(), spot_price)?;

        Ok(())
    }

    pub fn set_trigger_order(
        &self,
        ctx: &mut StateContext,
        id: PositionId,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<()> {
        let price = self.spot_price(ctx.storage, None)?;
        let mut pos = get_position(ctx.storage, id)?;
        let market_type = self.market_id(ctx.storage)?.get_market_type();

        pos.stop_loss_override = stop_loss_override;
        pos.stop_loss_override_notional =
            stop_loss_override.map(|x| x.into_notional_price(market_type));
        pos.take_profit_override = take_profit_override;
        pos.take_profit_override_notional =
            take_profit_override.map(|x| x.into_notional_price(market_type));

        // We need to validate the trigger prices against up-to-date information
        // based on a liquifunding, so we perform a liquifunding right now. We
        // then validate the updated position.
        let last_liquifund = pos.liquifunded_at;
        match self.position_liquifund(ctx, pos, last_liquifund, self.now(), false)? {
            MaybeClosedPosition::Open(mut pos) => {
                self.position_validate_trigger_orders(&pos, market_type, price)?;
                self.position_save(ctx, &mut pos, &price, true, false)?;
            }
            MaybeClosedPosition::Close(_) => anyhow::bail!("Cannot update trigger orders since the position will be closed on next liquifunding"),
        }

        Ok(())
    }
}
