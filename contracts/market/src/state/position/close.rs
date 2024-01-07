use crate::state::position::CLOSED_POSITIONS;
use crate::state::{position::CLOSED_POSITION_HISTORY, *};
use anyhow::Context;
use msg::contracts::market::delta_neutrality_fee::DeltaNeutralityFeeReason;
use msg::contracts::market::position::{events::PositionCloseEvent, Position};
use msg::contracts::market::position::{
    ClosePositionInstructions, ClosedPosition, MaybeClosedPosition, PositionCloseReason,
};
use shared::prelude::*;

impl State<'_> {
    pub(crate) fn close_position_via_msg(
        &self,
        ctx: &mut StateContext,
        pos: Position,
        settlement_price: PricePoint,
    ) -> Result<()> {
        let starts_at = pos.liquifunded_at;
        let ends_at = settlement_price.timestamp;
        // Confirm that all past liquifundings have been performed before explicitly closing
        debug_assert!(pos.next_liquifunding >= ends_at);
        let mcp = self.position_liquifund(ctx, pos, starts_at, ends_at, false)?;

        // Liquifunding may have triggered a close, so check before we close again
        let instructions = match mcp {
            MaybeClosedPosition::Open(pos) => ClosePositionInstructions {
                pos,
                capped_exposure: Signed::<Collateral>::zero(),
                additional_losses: Collateral::zero(),
                reason: PositionCloseReason::Direct,
                settlement_price,
                closed_during_liquifunding: false,
            },
            MaybeClosedPosition::Close(instructions) => instructions,
        };
        self.close_position(ctx, instructions)
    }

    /// called directly or from liquifund
    ///
    /// This function takes in override values for active_collateral and
    /// counter_collateral. The values within Position are required to be
    /// non-zero, but when closing a position both values can end up as 0.
    pub(crate) fn close_position(
        &self,
        ctx: &mut StateContext,
        ClosePositionInstructions {
            mut pos,
            capped_exposure,
            additional_losses,
            settlement_price,
            reason,
            closed_during_liquifunding,
        }: ClosePositionInstructions,
    ) -> Result<()> {
        if closed_during_liquifunding {
            // If the position was closed during liquifunding, then liquifunded_at will still be the previous value.
            debug_assert!(pos.liquifunded_at <= settlement_price.timestamp);
        } else {
            // The position was not closed during liquifunding. In this case, we need to ensure that we're
            // fully liquifunded up until the current price point.
            debug_assert_eq!(pos.liquifunded_at, settlement_price.timestamp);
        }
        // How much notional size are we undoing? Used for delta neutrality fee
        // and adjusting the open interest
        let notional_size_return = -pos.notional_size;

        // Pay out delta neutrality fee. Since this can potentially bring active
        // collateral down to 0 (but not further), we'll do the update to our
        // local value, not the active_collateral on the position.
        debug_assert!(pos.active_collateral.raw() >= pos.liquidation_margin.delta_neutrality);
        debug_assert!(
            pos.active_collateral.into_signed() + capped_exposure
                >= pos.liquidation_margin.delta_neutrality.into_signed()
        );
        let delta_neutrality_fee = self
            .charge_delta_neutrality_fee_no_update(
                ctx.storage,
                &pos,
                notional_size_return,
                &settlement_price,
                DeltaNeutralityFeeReason::PositionClose,
            )?
            .store(self, ctx)?;
        pos.add_delta_neutrality_fee(delta_neutrality_fee, &settlement_price)?;

        // Reduce net open interest. This needs to happen _after_ delta
        // neutrality fee payments so the slippage calculations are correct.
        self.adjust_net_open_interest(ctx, notional_size_return, pos.direction(), false)?;

        // Calculate the final active and counter collateral based on price
        // settlement exposure change and final delta neutrality fee payment.
        anyhow::ensure!(
            -capped_exposure <= pos.active_collateral.into_signed(),
            "Calculated exposure is {capped_exposure}, which outweighs active collateral of {}",
            pos.active_collateral
        );
        anyhow::ensure!(
            capped_exposure <= pos.counter_collateral.into_signed(),
            "Calculated exposure is {capped_exposure}, which outweighs counter collateral of {}",
            pos.counter_collateral
        );

        // Take the DNF out of the active collateral
        let active_collateral = pos
            .active_collateral
            .into_signed()
            .checked_sub(delta_neutrality_fee)?;
        anyhow::ensure!(active_collateral.is_positive_or_zero());

        // The final exposure needs to include all the additional losses that we
        // can provide funds for. So we calculate the total exposure (capped - additional
        // losses), and then make sure it doesn't exceed the active collateral after paying
        // all fees.
        let final_exposure = capped_exposure
            .checked_sub(additional_losses.into_signed())?
            .max(-active_collateral);

        // And now that we have the final exposure amount, we need to calculate
        // how much additional losses we just realized and update the locked liquidity in
        // the system to represent the additional funds sent to the liquidity pool.
        //
        // Take the exposure we already capped and subtract out the final exposure. Since both numbers in a loss scenario will be negative, this will give back the positive value representing the funds to be sent to the liquidity pool.
        let additional_lp_funds = capped_exposure.checked_sub(final_exposure)?;
        debug_assert!(additional_lp_funds >= Signed::zero());
        self.liquidity_update_locked(ctx, additional_lp_funds, &settlement_price)?;

        // Final active collateral is the active collateral post fees plus final
        // exposure numbers. The final exposure will be negative for losses and positive
        // for gains, thus the reason we add.
        let active_collateral = active_collateral
            .checked_add(final_exposure)?
            .try_into_non_negative_value()
            .with_context(|| {
                format!(
                    "close_position: negative active collateral: {} with exposure {}",
                    pos.active_collateral, final_exposure
                )
            })?;

        let active_collateral_usd = settlement_price.collateral_to_usd(active_collateral);
        let counter_collateral = pos
            .counter_collateral
            .into_signed()
            .checked_sub(final_exposure)?
            .try_into_non_negative_value()
            .with_context(|| {
                format!(
                    "close_position: negative counter collateral: {} with exposure {}",
                    pos.counter_collateral, final_exposure
                )
            })?;

        // unlock the LP collateral
        if let Some(counter_collateral) = NonZero::new(counter_collateral) {
            self.liquidity_unlock(ctx, counter_collateral, &settlement_price)?;
        }

        // send the trader's collateral to their wallet
        if let Some(active_collateral) = NonZero::new(active_collateral) {
            self.add_token_transfer_msg(ctx, &pos.owner, active_collateral)?;
        }

        // remove position from open list
        self.position_remove(ctx, pos.id)?;

        let market_id = self.market_id(ctx.storage)?;
        let market_type = market_id.get_market_type();

        let direction_to_base = pos.direction().into_base(market_type);
        let entry_price_base = match self.spot_price(
            ctx.storage,
            pos.price_point_created_at.unwrap_or(pos.created_at),
        ) {
            Ok(entry_price) => entry_price,
            Err(err) => return Err(err),
        }
        .price_base;
        let close_time = self.now();
        let closed_position = ClosedPosition {
            owner: pos.owner,
            id: pos.id,
            direction_to_base,
            created_at: pos.created_at,
            price_point_created_at: pos.price_point_created_at,
            liquifunded_at: pos.liquifunded_at,
            trading_fee_collateral: pos.trading_fee.collateral(),
            trading_fee_usd: pos.trading_fee.usd(),
            funding_fee_collateral: pos.funding_fee.collateral(),
            funding_fee_usd: pos.funding_fee.usd(),
            borrow_fee_collateral: pos.borrow_fee.collateral(),
            borrow_fee_usd: pos.borrow_fee.usd(),
            crank_fee_collateral: pos.crank_fee.collateral(),
            crank_fee_usd: pos.crank_fee.usd(),
            deposit_collateral: pos.deposit_collateral.collateral(),
            deposit_collateral_usd: pos.deposit_collateral.usd(),
            pnl_collateral: active_collateral
                .into_signed()
                .checked_sub(pos.deposit_collateral.collateral())?,
            pnl_usd: active_collateral_usd
                .into_signed()
                .checked_sub(pos.deposit_collateral.usd())?,
            notional_size: pos.notional_size,
            entry_price_base,
            close_time,
            settlement_time: settlement_price.timestamp,
            reason,
            active_collateral,
            delta_neutrality_fee_collateral: pos.delta_neutrality_fee.collateral(),
            delta_neutrality_fee_usd: pos.delta_neutrality_fee.usd(),
        };

        self.position_history_add_close(
            ctx,
            &closed_position,
            delta_neutrality_fee,
            &settlement_price,
        )?;

        self.nft_burn(ctx, &closed_position.owner, pos.id.to_string())?;

        CLOSED_POSITION_HISTORY.save(
            ctx.storage,
            (&closed_position.owner, (close_time, pos.id)),
            &closed_position,
        )?;

        CLOSED_POSITIONS.save(ctx.storage, pos.id, &closed_position)?;

        ctx.response_mut()
            .add_event(PositionCloseEvent { closed_position });

        Ok(())
    }

    /// Load a closed position by ID, if available
    pub(crate) fn load_closed_position(
        &self,
        store: &dyn Storage,
        pos_id: PositionId,
    ) -> Result<Option<ClosedPosition>> {
        CLOSED_POSITIONS
            .may_load(store, pos_id)
            .map_err(|e| e.into())
    }
}
