use crate::state::*;
use cosmwasm_std::Order;
use cw_storage_plus::{Bound, PrefixBound};
use msg::contracts::market::{
    crank::{events::CrankExecBatchEvent, CrankWorkInfo},
    position::{
        events::PositionSaveReason, ClosePositionInstructions, MaybeClosedPosition,
        PositionCloseReason,
    },
};

use shared::prelude::*;

use super::position::{
    get_position, LIQUIDATION_PRICES_PENDING_COUNT, NEXT_LIQUIFUNDING, NEXT_STALE, OPEN_POSITIONS,
};

/// The last price point timestamp for which the cranking process was completed.
///
/// If this is unavailable, we've never completed cranking, and we should find
/// the very first price timestamp.
pub(super) const LAST_CRANK_COMPLETED: Item<Timestamp> = Item::new(namespace::LAST_CRANK_COMPLETED);

pub(crate) fn crank_init(store: &mut dyn Storage, env: &Env) -> Result<()> {
    LAST_CRANK_COMPLETED
        .save(store, &env.block.time.into())
        .map_err(|err| err.into())
}

impl State<'_> {
    /// Get the next price timestamp that we need to perform cranking on.
    pub(crate) fn next_crank_timestamp(&self, store: &dyn Storage) -> Result<Option<PricePoint>> {
        let min = LAST_CRANK_COMPLETED.may_load(store)?.map(Bound::exclusive);
        self.spot_price_after(store, min)
    }

    /// Has the crank finished computations for all previous price points?
    ///
    /// If this is the case, the protocol is up-to-date and we can directly add
    /// liquidation prices to the liquidation data structures, bypassing the
    /// unpending queue.
    pub(crate) fn is_crank_up_to_date(&self, store: &dyn Storage) -> Result<bool> {
        Ok(match self.next_crank_timestamp(store)? {
            // No price point exists after the last crank completed, therefore
            // we're fully up to date
            None => true,
            Some(next_crank_timestamp) => {
                // There is a price point that still needs to be calculated. We
                // never crank the very latest price point, so check if there's
                // a newer price point. If there is, we're falling behind on the
                // crank. If the next price update available is the most recent
                // price update: we're fully up to date.
                let latest_price = self.spot_price_latest_opt(store)?;
                match latest_price {
                    // This should really never happen since we know there's at
                    // least one price point available by the return from
                    // next_crank_timestamp. But if it does, we'll default to
                    // being not up to date.
                    None => false,
                    // Only consider the crank up to date if the two timestamps
                    // match.
                    Some(latest_price) => latest_price.timestamp == next_crank_timestamp.timestamp,
                }
            }
        })
    }

    pub(crate) fn crank_work(&self, store: &dyn Storage) -> Result<Option<CrankWorkInfo>> {
        if self.get_close_all_positions(store)? {
            if let Some(position) = OPEN_POSITIONS
                .keys(store, None, None, Order::Ascending)
                .next()
                .transpose()?
            {
                return Ok(Some(CrankWorkInfo::CloseAllPositions { position }));
            }
        }

        if self.should_reset_lp_balances(store)? {
            return Ok(Some(CrankWorkInfo::ResetLpBalances {}));
        }

        Ok(match self.next_crank_timestamp(store)? {
            None => None,
            Some(price_point) => Some({
                if let Some(((_, position), _)) = NEXT_LIQUIFUNDING
                    .prefix_range(
                        store,
                        None,
                        Some(PrefixBound::inclusive(price_point.timestamp)),
                        cosmwasm_std::Order::Ascending,
                    )
                    .next()
                    .transpose()?
                {
                    CrankWorkInfo::Liquifunding { position }
                } else if let Some(position) =
                    self.pending_liquidation_prices(store, price_point.timestamp)?
                {
                    CrankWorkInfo::UnpendLiquidationPrices { position }
                } else if let Some(pos) =
                    self.liquidatable_position(store, price_point.price_notional)?
                {
                    CrankWorkInfo::Liquidation {
                        position: pos.id,
                        liquidation_reason: pos.reason,
                        price_point,
                    }
                } else if let Some(order_id) =
                    self.limit_order_triggered_order(store, price_point.price_notional, false)?
                {
                    CrankWorkInfo::LimitOrder { order_id }
                } else {
                    CrankWorkInfo::Completed {
                        price_point_timestamp: price_point.timestamp,
                    }
                }
            }),
        })
    }

    /// Would the given price update trigger any liquidations?
    pub(crate) fn price_would_trigger(
        &self,
        store: &dyn Storage,
        price: PriceBaseInQuote,
    ) -> Result<bool> {
        let price = price.into_notional_price(self.market_type(store)?);
        if self.liquidatable_position(store, price)?.is_some() {
            return Ok(true);
        }
        self.limit_order_triggered_order(store, price, true)
            .map(|x| x.is_some())
    }

    // this always executes the requested cranks
    // if there is no work to be done, then crank_exec itself will be cheap
    // QueryMsg::CrankStats can be used by clients to get heuristics and decide how many to crank
    pub fn crank_exec_batch(
        &self,
        ctx: &mut StateContext,
        n_execs: Option<u32>,
        rewards: &Addr,
    ) -> Result<()> {
        let n_execs = match n_execs {
            None => self.config.crank_execs,
            Some(n) => n,
        }
        .into();

        let starting_unpend = LIQUIDATION_PRICES_PENDING_COUNT
            .may_load(ctx.storage)?
            .unwrap_or_default();

        let mut actual = vec![];
        for _ in 0..n_execs {
            match self.crank_work(ctx.storage)? {
                None => break,
                Some(work_info) => {
                    actual.push(work_info.clone());
                    self.crank_exec(ctx, work_info)?;
                }
            };
        }

        let ending_unpend = LIQUIDATION_PRICES_PENDING_COUNT
            .may_load(ctx.storage)?
            .unwrap_or_default();

        self.allocate_crank_fees(ctx, rewards, actual.len().try_into()?)?;
        ctx.response_mut().add_event(CrankExecBatchEvent {
            requested: n_execs,
            actual,
            starting_unpend,
            ending_unpend,
        });

        Ok(())
    }

    /// If the next crank work items completes a price update, crank it.
    ///
    /// This is a special optimization to avoid accruing unnecessary "complete
    /// work" items and causing the unpend queue to fill up.
    /// TBD: is this still needed?
    // pub(crate) fn crank_current_price_complete(&self, ctx: &mut StateContext) -> Result<()> {
    //     let work_info = match self.crank_work(ctx.storage)? {
    //         Some(work_info) => work_info,
    //         None => return Ok(()),
    //     };
    //     let price_point_timestamp = match &work_info {
    //         CrankWorkInfo::Completed {
    //             price_point_timestamp,
    //         } => *price_point_timestamp,
    //         _ => return Ok(()),
    //     };

    //     let current = self.spot_price(ctx.storage, None)?;

    //     if price_point_timestamp == current.timestamp {
    //         // Finish off the price update
    //         self.crank_exec(ctx, work_info)?;
    //     }

    //     Ok(())
    // }

    /// What is the ending timestamp for liquifunding and liquidation?
    ///
    /// Generally speaking, the crank performs its actions up until "now." That
    /// means liquifunding does fee calculations up until the current timestamp,
    /// and liquidations occur as of the current timestamp. The one exception to
    /// this is when the protocol is stale. In that case, we do not have well
    /// fundedness guarantees beyond the point where the protocol became stale,
    /// and we therefore calculate up until the protocol entered stale.
    ///
    /// This function checks if the protocol is stale and, if so, returns that
    /// timestamp. Otherwise it returns now.
    fn stale_or_now(&self, store: &dyn Storage) -> Result<Timestamp> {
        let now = self.now();
        Ok(
            match NEXT_STALE
                .keys(store, None, None, cosmwasm_std::Order::Ascending)
                .next()
                .transpose()?
            {
                Some((stale, _)) if now > stale => stale,
                _ => now,
            },
        )
    }

    /// Perform a single crank execution.
    fn crank_exec(&self, ctx: &mut StateContext, work_info: CrankWorkInfo) -> Result<()> {
        // get our current playhead time and price for liquidations
        ctx.response_mut().add_event(work_info.clone());

        // do the work
        match work_info {
            CrankWorkInfo::CloseAllPositions { position } => {
                let pos = get_position(ctx.storage, position)?;
                self.close_position_via_msg(ctx, pos)?;
            }
            CrankWorkInfo::ResetLpBalances {} => {
                self.crank_reset_lp_balances(ctx)?;
            }
            CrankWorkInfo::Liquifunding { position } => {
                let pos = get_position(ctx.storage, position)?;
                let starts_at = pos.liquifunded_at;
                let ends_at = pos.next_liquifunding;
                self.position_liquifund_store(
                    ctx,
                    pos,
                    starts_at,
                    ends_at,
                    true,
                    PositionSaveReason::Crank,
                )?;
            }
            CrankWorkInfo::UnpendLiquidationPrices { position } => {
                self.unpend_liquidation_prices(ctx, position)?;
            }
            CrankWorkInfo::Liquidation {
                position,
                liquidation_reason,
                price_point,
            } => {
                let pos = get_position(ctx.storage, position)?;

                let starts_at = pos.liquifunded_at;
                let ends_at = self.stale_or_now(ctx.storage)?;
                let mcp = self.position_liquifund(ctx, pos, starts_at, ends_at, true)?;

                let close_position_instructions = match mcp {
                    MaybeClosedPosition::Open(pos) => ClosePositionInstructions {
                        pos,
                        exposure: Signed::zero(),
                        close_time: ends_at,
                        settlement_time: price_point.timestamp,
                        reason: PositionCloseReason::Liquidated(liquidation_reason),
                    },
                    MaybeClosedPosition::Close(x) => x,
                };
                self.close_position(ctx, close_position_instructions)?;
            }
            CrankWorkInfo::LimitOrder { order_id } => {
                self.limit_order_execute_order(ctx, order_id)?;
            }
            CrankWorkInfo::Completed {
                price_point_timestamp,
            } => {
                self.accumulate_funding_rate(ctx, price_point_timestamp)?;
                LAST_CRANK_COMPLETED.save(ctx.storage, &price_point_timestamp)?;
            }
        }

        Ok(())
    }
}
