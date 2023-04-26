use crate::prelude::*;
use msg::contracts::market::position::{
    ClosePositionInstructions, LiquidationReason, MaybeClosedPosition, PositionCloseReason,
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
    ) -> Result<()> {
        let mcp = self.position_liquifund(ctx, pos, starts_at, ends_at, charge_crank_fee)?;
        self.process_maybe_closed_position(ctx, mcp, ends_at)
    }

    pub(crate) fn process_maybe_closed_position(
        &self,
        ctx: &mut StateContext,
        mcp: MaybeClosedPosition,
        ends_at: Timestamp,
    ) -> Result<()> {
        match mcp {
            MaybeClosedPosition::Open(mut position) => {
                let price_point = self.spot_price(ctx.storage, Some(ends_at))?;
                self.position_save(ctx, &mut position, &price_point, true, false)?;
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
        let start_price = self.spot_price(ctx.storage, Some(starts_at))?;
        let end_price = self.spot_price(ctx.storage, Some(ends_at))?;
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
                return Ok(MaybeClosedPosition::Close(instructions))
            }
        };

        let slippage_liquidation_margin = pos.liquidation_margin.delta_neutrality;
        let (mcp, exposure) = pos.settle_price_exposure(
            start_price.price_notional,
            end_price.price_notional,
            // Make sure we have at least enough funds set aside for delta
            // neutrality fee when closing.
            slippage_liquidation_margin,
            ends_at,
        )?;

        self.liquidity_update_locked(ctx, -exposure)?;

        let mut pos = match mcp {
            MaybeClosedPosition::Open(pos) => pos,
            MaybeClosedPosition::Close(instructions) => {
                return Ok(MaybeClosedPosition::Close(instructions))
            }
        };

        // After settlement, a position might need to be liquidated because the position does not
        // have enough collateral left to cover the liquidation margin for the upcoming period.
        let liquidation_margin = pos.liquidation_margin(
            end_price.price_notional,
            &self.spot_price(ctx.storage, None)?,
            config,
        )?;
        if pos.active_collateral.raw() <= liquidation_margin.total() {
            return Ok(MaybeClosedPosition::Close(ClosePositionInstructions {
                pos,
                // Exposure is 0 here: we've already added in the exposure
                // value from settling above.
                exposure: Signed::zero(),
                close_time: ends_at,
                settlement_time: ends_at,
                reason: PositionCloseReason::Liquidated(LiquidationReason::Liquidated),
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
                exposure: Signed::zero(),
                close_time: ends_at,
                settlement_time: ends_at,
                reason: PositionCloseReason::Liquidated(LiquidationReason::MaxGains),
            }));
        };

        // Position does not need to be closed
        pos.liquifunded_at = ends_at;
        pos.next_liquifunding = ends_at.plus_seconds(config.liquifunding_delay_seconds.into());
        pos.stale_at = pos
            .next_liquifunding
            .plus_seconds(config.staleness_seconds.into());
        pos.liquidation_margin = liquidation_margin;
        Ok(MaybeClosedPosition::Open(pos))
    }
}
