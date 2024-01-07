use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use crate::prelude::*;
use msg::contracts::market::position::{
    events::PositionSaveReason, ClosePositionInstructions, LiquidationReason, MaybeClosedPosition,
    PositionCloseReason,
};

impl State<'_> {
    /// Same as [State::position_liquifund], but update the stored data with the resulting [MaybeClosedPosition].
    pub(crate) fn position_liquifund_store(
        &self,
        ctx: &mut StateContext,
        pos: Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
        charge_crank_fee: bool,
        reason: PositionSaveReason,
    ) -> Result<()> {
        let mcp = self.position_liquifund(ctx, pos, starts_at, ends_at, charge_crank_fee)?;
        self.process_maybe_closed_position(ctx, mcp, ends_at, reason)
    }

    fn process_maybe_closed_position(
        &self,
        ctx: &mut StateContext,
        mcp: MaybeClosedPosition,
        ends_at: Timestamp,
        reason: PositionSaveReason,
    ) -> Result<()> {
        match mcp {
            MaybeClosedPosition::Open(mut position) => {
                let price_point = self.spot_price(ctx.storage, ends_at)?;
                self.position_save(ctx, &mut position, &price_point, true, false, reason)?;
                Ok(())
            }
            MaybeClosedPosition::Close(close_position_instructions) => {
                self.close_position(ctx, close_position_instructions)
            }
        }
    }

    pub(crate) fn position_liquifund(
        &self,
        ctx: &mut StateContext,
        pos: Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
        charge_crank_fee: bool,
    ) -> Result<MaybeClosedPosition> {
        debug_assert!(starts_at <= ends_at);
        debug_assert!(starts_at == pos.liquifunded_at);
        debug_assert!(pos.next_liquifunding >= ends_at);

        let start_price = self.spot_price(ctx.storage, starts_at)?;
        let end_price = self.spot_price(ctx.storage, ends_at)?;
        let config = &self.config;

        // PERP-996 we don't allow the liquifunding process to flip
        // direction-to-base by reducing leverage too far. Capture the initial
        // direction.
        let market_type = self.market_type(ctx.storage)?;
        let original_direction_to_base = pos
            .active_leverage_to_notional(&end_price)
            .into_base(market_type)
            .split()
            .0;

        let pos = match self.position_settle_pending_fees(
            ctx,
            pos,
            starts_at,
            ends_at,
            charge_crank_fee,
        )? {
            MaybeClosedPosition::Open(pos) => pos,
            MaybeClosedPosition::Close(instructions) => {
                return Ok(MaybeClosedPosition::Close(instructions));
            }
        };

        let slippage_liquidation_margin = pos.liquidation_margin.delta_neutrality;
        let (mcp, exposure) = pos.settle_price_exposure(
            start_price.price_notional,
            end_price,
            // Make sure we have at least enough funds set aside for delta
            // neutrality fee when closing.
            slippage_liquidation_margin,
        )?;

        self.liquidity_update_locked(ctx, -exposure, &end_price)?;

        let mut pos = match mcp {
            MaybeClosedPosition::Open(pos) => pos,
            MaybeClosedPosition::Close(instructions) => {
                return Ok(MaybeClosedPosition::Close(instructions))
            }
        };

        // After settlement, a position might need to be liquidated because the position does not
        // have enough collateral left to cover the liquidation margin for the upcoming period.
        let liquidation_margin = pos.liquidation_margin(&end_price, config)?;
        if pos.active_collateral.raw() <= liquidation_margin.total() {
            return Ok(MaybeClosedPosition::Close(ClosePositionInstructions {
                pos,
                // Exposure is 0 here: we've already added in the exposure
                // value from settling above.
                capped_exposure: Signed::zero(),
                additional_losses: Collateral::zero(),
                settlement_price: end_price,
                reason: PositionCloseReason::Liquidated(LiquidationReason::Liquidated),
                closed_during_liquifunding: true,
            }));
        }

        // PERP-996 make sure the direction hasn't changed. If it has: take max
        // gains at this point. If we don't take max gains here, our relative
        // exposure to the collateral asset means that even if the position
        // could stand to gain more _collateral_, the price change in the
        // collateral versus the quote asset will be extreme enough that the
        // trader will lose money.
        let new_direction_to_base = pos
            .active_leverage_to_notional(&end_price)
            .into_base(market_type)
            .split()
            .0;
        if original_direction_to_base != new_direction_to_base {
            return Ok(MaybeClosedPosition::Close(ClosePositionInstructions {
                pos,
                capped_exposure: Signed::zero(),
                additional_losses: Collateral::zero(),
                settlement_price: end_price,
                reason: PositionCloseReason::Liquidated(LiquidationReason::MaxGains),
                closed_during_liquifunding: true,
            }));
        };

        // Position does not need to be closed
        self.set_next_liquifunding(&mut pos, ends_at);
        pos.liquidation_margin = liquidation_margin;
        Ok(MaybeClosedPosition::Open(pos))
    }

    /// Updates the liquifunded_at, next_liquifunding, and stale_at fields of the position.
    ///
    /// Includes logic for randomization of the next_liquifunding field
    pub(crate) fn set_next_liquifunding(&self, pos: &mut Position, liquifunded_at: Timestamp) {
        // First set up the values correctly
        pos.liquifunded_at = liquifunded_at;
        pos.next_liquifunding =
            liquifunded_at.plus_seconds(self.config.liquifunding_delay_seconds.into());

        if self.config.liquifunding_delay_fuzz_seconds != 0 {
            // Next we're going to add a bit of randomization to schedule the
            // next_liquifunding earlier than it should be. This is part of the
            // crank smoothing mechanism. We don't need true randomness for this,
            // just something random enough to spread load around.
            let mut hash = DefaultHasher::new();
            self.now().hash(&mut hash);
            pos.owner.hash(&mut hash);
            pos.id.hash(&mut hash);
            let semi_random = hash.finish();

            // If anything goes wrong, we just ignore it. This is an optimization that is
            // allowed to fail.
            let res = (|| {
                let how_early_seconds =
                    semi_random.checked_rem(self.config.liquifunding_delay_fuzz_seconds.into())?;
                let actual_delay = u64::from(self.config.liquifunding_delay_seconds)
                    .checked_sub(how_early_seconds)?;
                pos.next_liquifunding = liquifunded_at.plus_seconds(actual_delay);

                Some(())
            })();
            debug_assert_eq!(res, Some(()));
        }
    }
}
