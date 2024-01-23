use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use crate::{
    prelude::*,
    state::{funding::PositionFeeSettlement, liquidity::LiquidityUpdateLocked},
};
use msg::contracts::market::position::{
    events::PositionSaveReason, ClosePositionInstructions, LiquidationReason, MaybeClosedPosition,
    PositionCloseReason,
};

impl State<'_> {
    /// creates a (validated) [PositionLiquifund], stores it, and processes the resulting [MaybeClosedPosition].
    pub(crate) fn position_liquifund_store(
        &self,
        ctx: &mut StateContext,
        pos: Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
        charge_crank_fee: bool,
        reason: PositionSaveReason,
    ) -> Result<()> {
        let liquifund =
            PositionLiquifund::new(self, ctx.storage, pos, starts_at, ends_at, charge_crank_fee)?;
        liquifund.apply(self, ctx)?;
        self.process_maybe_closed_position(ctx, liquifund.position, ends_at, reason)
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

#[must_use]
pub(crate) struct PositionLiquifund {
    pub fee_settlement: PositionFeeSettlement,
    pub position: MaybeClosedPosition,
    pub liquidity_update_locked: Option<LiquidityUpdateLocked>,
}

impl PositionLiquifund {
    pub fn new(
        state: &State,
        store: &dyn Storage,
        pos: Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
        charge_crank_fee: bool,
    ) -> Result<Self> {
        debug_assert!(starts_at <= ends_at);
        debug_assert!(starts_at == pos.liquifunded_at);
        debug_assert!(pos.next_liquifunding >= ends_at);

        let start_price = state.spot_price(store, starts_at)?;
        let end_price = state.spot_price(store, ends_at)?;
        let config = &state.config;

        // PERP-996 we don't allow the liquifunding process to flip
        // direction-to-base by reducing leverage too far. Capture the initial
        // direction.
        let market_type = state.market_type(store)?;
        let original_direction_to_base = pos
            .active_leverage_to_notional(&end_price)
            .into_base(market_type)
            .split()
            .0;

        let pos_fee_settlement =
            PositionFeeSettlement::new(state, store, pos, starts_at, ends_at, charge_crank_fee)?;

        let pos = match &pos_fee_settlement.position {
            MaybeClosedPosition::Open(pos) => pos,
            MaybeClosedPosition::Close(instructions) => {
                let instructions = instructions.clone();
                return Ok(Self {
                    fee_settlement: pos_fee_settlement,
                    position: MaybeClosedPosition::Close(instructions),
                    liquidity_update_locked: None,
                });
            }
        }
        .clone();

        let slippage_liquidation_margin = pos.liquidation_margin.delta_neutrality;
        let (mcp, exposure) = pos.settle_price_exposure(
            start_price.price_notional,
            end_price,
            // Make sure we have at least enough funds set aside for delta
            // neutrality fee when closing.
            slippage_liquidation_margin,
        )?;

        let liquidity_update_locked = LiquidityUpdateLocked {
            amount: -exposure,
            price: end_price,
        };
        liquidity_update_locked.validate(state, store)?;
        let liquidity_update_locked = Some(liquidity_update_locked);

        let mut pos = match mcp {
            MaybeClosedPosition::Open(pos) => pos,
            MaybeClosedPosition::Close(instructions) => {
                return Ok(Self {
                    fee_settlement: pos_fee_settlement,
                    position: MaybeClosedPosition::Close(instructions),
                    liquidity_update_locked,
                })
            }
        };

        // After settlement, a position might need to be liquidated because the position does not
        // have enough collateral left to cover the liquidation margin for the upcoming period.
        let liquidation_margin = pos.liquidation_margin(&end_price, config)?;
        if pos.active_collateral.raw() <= liquidation_margin.total() {
            let position = MaybeClosedPosition::Close(ClosePositionInstructions {
                pos,
                // Exposure is 0 here: we've already added in the exposure
                // value from settling above.
                capped_exposure: Signed::zero(),
                additional_losses: Collateral::zero(),
                settlement_price: end_price,
                reason: PositionCloseReason::Liquidated(LiquidationReason::Liquidated),
                closed_during_liquifunding: true,
            });

            return Ok(Self {
                fee_settlement: pos_fee_settlement,
                position,
                liquidity_update_locked,
            });
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
            let position = MaybeClosedPosition::Close(ClosePositionInstructions {
                pos,
                capped_exposure: Signed::zero(),
                additional_losses: Collateral::zero(),
                settlement_price: end_price,
                reason: PositionCloseReason::Liquidated(LiquidationReason::MaxGains),
                closed_during_liquifunding: true,
            });
            return Ok(Self {
                fee_settlement: pos_fee_settlement,
                position,
                liquidity_update_locked,
            });
        };

        // Position does not need to be closed
        state.set_next_liquifunding(&mut pos, ends_at);
        pos.liquidation_margin = liquidation_margin;

        Ok(Self {
            fee_settlement: pos_fee_settlement,
            position: MaybeClosedPosition::Open(pos),
            liquidity_update_locked,
        })
    }
    pub fn apply(&self, state: &State, ctx: &mut StateContext) -> Result<()> {
        self.fee_settlement.apply(state, ctx)?;
        if let Some(liquidity_update_locked) = &self.liquidity_update_locked {
            liquidity_update_locked.apply(state, ctx)?;
        }
        Ok(())
    }
}
