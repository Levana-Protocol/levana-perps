use crate::state::liquidity::{LiquidityUnlock, LiquidityUpdateLocked};
use crate::state::position::liquifund::PositionLiquifund;
use crate::state::position::{AdjustOpenInterest, CLOSED_POSITIONS};
use crate::state::{position::CLOSED_POSITION_HISTORY, *};
use anyhow::Context;
use msg::contracts::market::delta_neutrality_fee::DeltaNeutralityFeeReason;
use msg::contracts::market::position::{events::PositionCloseEvent, Position};
use msg::contracts::market::position::{
    ClosePositionInstructions, ClosedPosition, MaybeClosedPosition, PositionCloseReason,
};
use shared::prelude::*;

use self::delta_neutrality_fee::ChargeDeltaNeutralityFeeResult;

impl State<'_> {
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

#[must_use]
pub(crate) struct ClosePositionExec {
    dnf: ChargeDeltaNeutralityFeeResult,
    open_interest: AdjustOpenInterest,
    liquidity_update: LiquidityUpdateLocked,
    liquidity_unlock: Option<LiquidityUnlock>,
    trader_collateral_to_send: Option<NonZero<Collateral>>,
    closed_position: ClosedPosition,
    settlement_price: PricePoint,
    // Prior to the "deferred error recovery" requirements, we simply did liquifunding
    // in the message handler and then closing continued on from there.
    // There was no need for the close position process itself to know about liquifunding.
    //
    // Since we now merely build up the liquifunding struct and defer applying it, we need to
    // pass it down for two reasons:
    // 1. Have it available for the ClosePositionExec to apply it
    // 2. We need to extract the LiquidityStats, so that we can pick up from there
    //    in other words, what the liquidity stats will be after the liquifunding is applied
    //    and before LiquidityUnlock is applied (LiquidityUnlock happens to be the first part
    //    of the close position process that uses the liquidity stats)
    liquifund_via_close_msg: Option<PositionLiquifund>,
}

impl ClosePositionExec {
    pub(crate) fn new_via_msg(
        state: &State,
        store: &dyn Storage,
        pos: Position,
        settlement_price: PricePoint,
    ) -> Result<Self> {
        let starts_at = pos.liquifunded_at;
        let ends_at = settlement_price.timestamp;
        // Confirm that all past liquifundings have been performed before explicitly closing
        debug_assert!(pos.next_liquifunding >= ends_at);
        let liquifund = PositionLiquifund::new(state, store, pos, starts_at, ends_at, false)?;

        let instructions = match &liquifund.position {
            MaybeClosedPosition::Open(pos) => ClosePositionInstructions {
                pos: pos.clone(),
                capped_exposure: Signed::<Collateral>::zero(),
                additional_losses: Collateral::zero(),
                reason: PositionCloseReason::Direct,
                settlement_price,
                closed_during_liquifunding: false,
            },
            MaybeClosedPosition::Close(instructions) => instructions.clone(),
        };

        Self::new(state, store, instructions, Some(liquifund))
    }

    /// called directly or from liquifund
    ///
    /// This function takes in override values for active_collateral and
    /// counter_collateral. The values within Position are required to be
    /// non-zero, but when closing a position both values can end up as 0.
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        ClosePositionInstructions {
            mut pos,
            capped_exposure,
            additional_losses,
            settlement_price,
            reason,
            closed_during_liquifunding,
        }: ClosePositionInstructions,
        liquifund_via_close_msg: Option<PositionLiquifund>,
    ) -> Result<Self> {
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
        let dnf = state.charge_delta_neutrality_fee_no_update(
            store,
            &pos,
            notional_size_return,
            &settlement_price,
            DeltaNeutralityFeeReason::PositionClose,
        )?;

        pos.add_delta_neutrality_fee(dnf.fee, &settlement_price)?;

        // TBD: retaining previous comment, but it appears to be stale:
        // Reduce net open interest. This needs to happen _after_ delta
        // neutrality fee payments so the slippage calculations are correct.
        let open_interest =
            AdjustOpenInterest::new(state, store, notional_size_return, pos.direction(), false)?;

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
        let active_collateral = pos.active_collateral.into_signed().checked_sub(dnf.fee)?;
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

        let liquidity_update = LiquidityUpdateLocked::new(
            state,
            store,
            additional_lp_funds,
            settlement_price,
            liquifund_via_close_msg
                .as_ref()
                .and_then(|l| l.liquidity_update_locked.as_ref())
                .map(|l| l.stats.clone()),
        )?;

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
        let liquidity_unlock = match NonZero::new(counter_collateral) {
            None => None,
            Some(counter_collateral) => {
                // the storage that LiquidityUnlock reads from isn't actually updated from liquidity_update yet
                // so we need to pick up from LiquidityUpdateLocked.stats
                Some(LiquidityUnlock::new(
                    state,
                    store,
                    counter_collateral,
                    settlement_price,
                    Some(liquidity_update.stats.clone()),
                )?)
            }
        };

        let trader_collateral_to_send = NonZero::new(active_collateral);

        let market_id = state.market_id(store)?;
        let market_type = market_id.get_market_type();

        let direction_to_base = pos.direction().into_base(market_type);
        let entry_price_base =
            match state.spot_price(store, pos.price_point_created_at.unwrap_or(pos.created_at)) {
                Ok(entry_price) => entry_price,
                Err(err) => return Err(err),
            }
            .price_base;

        let close_time = state.now();

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
            liquidation_margin: Some(pos.liquidation_margin),
        };

        Ok(Self {
            liquifund_via_close_msg,
            dnf,
            open_interest,
            liquidity_update,
            liquidity_unlock,
            trader_collateral_to_send,
            closed_position,
            settlement_price,
        })
    }

    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}

    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<()> {
        if let Some(liquifund) = self.liquifund_via_close_msg {
            let _ = liquifund.apply(state, ctx)?;
        }
        let dnf_fee = self.dnf.fee;
        self.dnf.apply(state, ctx)?;
        self.open_interest.apply(ctx)?;
        self.liquidity_update.apply(state, ctx)?;
        if let Some(liquidity_unlock) = self.liquidity_unlock {
            liquidity_unlock.apply(state, ctx)?;
        }

        // send the trader's collateral to their wallet
        if let Some(collateral) = self.trader_collateral_to_send {
            state.add_token_transfer_msg(ctx, &self.closed_position.owner, collateral)?;
        }

        // remove position from open list
        state.position_remove(ctx, self.closed_position.id)?;

        state.position_history_add_close(
            ctx,
            &self.closed_position,
            dnf_fee,
            &self.settlement_price,
        )?;

        state.nft_burn(
            ctx,
            &self.closed_position.owner,
            self.closed_position.id.to_string(),
        )?;

        CLOSED_POSITION_HISTORY.save(
            ctx.storage,
            (
                &self.closed_position.owner,
                (self.closed_position.close_time, self.closed_position.id),
            ),
            &self.closed_position,
        )?;

        CLOSED_POSITIONS.save(ctx.storage, self.closed_position.id, &self.closed_position)?;

        ctx.response_mut().add_event(PositionCloseEvent {
            closed_position: self.closed_position,
        });
        Ok(())
    }
}
