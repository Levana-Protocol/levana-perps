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

use super::position::{get_position, NEXT_LIQUIFUNDING, OPEN_POSITIONS};

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

    fn get_close_all_positions_work(&self, store: &dyn Storage) -> Result<Option<PositionId>> {
        Ok(if self.get_close_all_positions(store)? {
            #[allow(clippy::manual_map)]
            if let Some(position) = OPEN_POSITIONS
                .keys(store, None, None, Order::Ascending)
                .next()
                .transpose()?
            {
                Some(position)
            } else {
                None
            }
        } else {
            None
        })
    }

    pub(crate) fn crank_work(&self, store: &dyn Storage) -> Result<Option<CrankWorkInfo>> {
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
                } else if let Some(position) = self.get_close_all_positions_work(store)? {
                    // We only try to close all positions _after_ we've done all
                    // liquifunding. We need to ensure that all positions are liquifunded up until the
                    // current price point before trying to close them.
                    CrankWorkInfo::CloseAllPositions {
                        position,
                        price_point,
                    }
                } else if let Some(pos) =
                    self.liquidatable_position(store, price_point.price_notional)?
                {
                    CrankWorkInfo::Liquidation {
                        position: pos.id,
                        liquidation_reason: pos.reason,
                        price_point,
                    }
                } else if let Some((deferred_exec_id, target)) =
                    self.next_crankable_deferred_exec_id(store, price_point.timestamp)?
                {
                    CrankWorkInfo::DeferredExec {
                        deferred_exec_id,
                        target,
                        price_point_timestamp: price_point.timestamp,
                    }
                } else if let Some(order_id) =
                    self.limit_order_triggered_order(store, price_point.price_notional, false)?
                {
                    CrankWorkInfo::LimitOrder {
                        order_id,
                        price_point,
                    }
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

        // Since deferred execution occurs in submessages, we cannot interleave
        // deferred execution work with other work items that will occur in the current
        // message. Therefore, once we see a deferred execution message, we do not process
        // any other kind of message.
        let mut saw_deferred_exec = false;

        let mut actual = vec![];
        let mut fees_earned = 0;
        for _ in 0..n_execs {
            match self.crank_work(ctx.storage)? {
                None => break,
                Some(work_info) => {
                    let is_deferred_exec = matches!(&work_info, CrankWorkInfo::DeferredExec { .. });
                    if !is_deferred_exec && saw_deferred_exec {
                        break;
                    }
                    saw_deferred_exec = saw_deferred_exec || is_deferred_exec;

                    actual.push(work_info.clone());
                    if work_info.receives_crank_rewards() {
                        fees_earned += 1;
                    }
                    self.crank_exec(ctx, work_info)?;
                }
            };
        }

        self.allocate_crank_fees(ctx, rewards, fees_earned)?;
        ctx.response_mut().add_event(CrankExecBatchEvent {
            requested: n_execs,
            actual,
        });

        Ok(())
    }

    /// If the next crank work items completes a price update, crank it.
    ///
    /// This is a special optimization to avoid accruing unnecessary "complete
    /// work" items and causing the unpend queue to fill up.
    pub(crate) fn crank_current_price_complete(&self, ctx: &mut StateContext) -> Result<()> {
        let work_info = match self.crank_work(ctx.storage)? {
            Some(work_info) => work_info,
            None => return Ok(()),
        };

        let price_point_timestamp = match &work_info {
            CrankWorkInfo::Completed {
                price_point_timestamp,
            } => *price_point_timestamp,
            _ => return Ok(()),
        };

        let current = self.current_spot_price(ctx.storage)?;

        if price_point_timestamp == current.timestamp {
            // Finish off the price update
            self.crank_exec(ctx, work_info)?;
        }

        Ok(())
    }

    /// Perform a single crank execution.
    fn crank_exec(&self, ctx: &mut StateContext, work_info: CrankWorkInfo) -> Result<()> {
        // get our current playhead time and price for liquidations
        ctx.response_mut().add_event(work_info.clone());

        // do the work
        match work_info {
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
            CrankWorkInfo::CloseAllPositions {
                position,
                price_point,
            } => {
                let pos = get_position(ctx.storage, position)?;
                self.close_position_via_msg(ctx, pos, price_point)?;
            }
            CrankWorkInfo::Liquidation {
                position,
                liquidation_reason,
                price_point,
            } => {
                let pos = get_position(ctx.storage, position)?;

                // Do one more liquifunding before closing the position to
                // pay out fees. This may end up closing the position on its own, otherwise we
                // explicitly close it ourselves because we hit a trigger.
                let starts_at = pos.liquifunded_at;

                // All positions that need to be liquifunded at this time _must_ have already be liquifunded.
                debug_assert!(pos.next_liquifunding >= price_point.timestamp);

                // We want to liquifund up until the price point's timestamp and make sure we shouldn't be liquidated for some other reason.
                let ends_at = price_point.timestamp;
                let mcp = self.position_liquifund(ctx, pos, starts_at, ends_at, true)?;

                let close_position_instructions = match mcp {
                    MaybeClosedPosition::Open(pos) => ClosePositionInstructions {
                        pos,
                        exposure: Signed::zero(),
                        settlement_price: price_point,
                        reason: PositionCloseReason::Liquidated(liquidation_reason),
                    },
                    MaybeClosedPosition::Close(x) => x,
                };
                self.close_position(ctx, close_position_instructions)?;
            }
            CrankWorkInfo::DeferredExec {
                deferred_exec_id,
                price_point_timestamp,
                target: _,
            } => {
                self.process_deferred_exec(ctx, deferred_exec_id, price_point_timestamp)?;
            }
            CrankWorkInfo::LimitOrder {
                order_id,
                price_point,
            } => {
                self.limit_order_execute_order(ctx, order_id, &price_point)?;
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
