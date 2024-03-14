use crate::state::*;
use cosmwasm_std::Order;
use cw_storage_plus::{Bound, PrefixBound};
use msg::contracts::market::{
    crank::{
        events::{CrankExecBatchEvent, CrankWorkInfoEvent},
        CrankWorkInfo,
    },
    position::{
        events::PositionSaveReason, ClosePositionInstructions, MaybeClosedPosition,
        PositionCloseReason,
    },
    spot_price::SpotPriceConfig,
};
use serde::{Deserialize, Serialize};

use shared::prelude::*;

use super::position::{get_position, NEXT_LIQUIFUNDING, OPEN_POSITIONS};

/// The last price point timestamp for which the cranking process was completed.
///
/// If this is unavailable, we've never completed cranking, and we should find
/// the very first price timestamp.
pub(super) const LAST_CRANK_COMPLETED: Item<Timestamp> = Item::new(namespace::LAST_CRANK_COMPLETED);
const CRANK_BATCH_WEIGHT_LEFT: Item<CrankProgress> = Item::new(namespace::CRANK_BATCH_WEIGHT_LEFT);

#[derive(Debug, Serialize, Deserialize)]
struct CrankProgress {
    weight_budget_left: u32,
    paying_work_done: u32,
    requested: u32,
    rewards: Addr,
    actual: Vec<(CrankWorkInfo, PricePoint)>,
}

pub(crate) fn crank_init(store: &mut dyn Storage) -> Result<()> {
    LAST_CRANK_COMPLETED
        .save(store, &Timestamp::from_seconds(0))
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

    /// Takes in a price_point from the next_crank_timestamp method
    pub(crate) fn crank_work(
        &self,
        store: &dyn Storage,
        price_point: PricePoint,
    ) -> Result<CrankWorkInfo> {
        if self.should_reset_lp_balances(store)? {
            return Ok(CrankWorkInfo::ResetLpBalances {});
        }

        Ok(
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
                CrankWorkInfo::CloseAllPositions { position }
            } else if let Some(pos) =
                self.liquidatable_position(store, price_point.price_notional)?
            {
                CrankWorkInfo::Liquidation {
                    position: pos.id,
                    liquidation_reason: pos.reason,
                }
            } else if let Some((deferred_exec_id, target)) =
                self.next_crankable_deferred_exec_id(store, price_point.timestamp)?
            {
                CrankWorkInfo::DeferredExec {
                    deferred_exec_id,
                    target,
                }
            } else if let Some(order_id) =
                self.limit_order_triggered_order(store, price_point.price_notional)?
            {
                CrankWorkInfo::LimitOrder { order_id }
            } else {
                CrankWorkInfo::Completed {}
            },
        )
    }

    /// Would the given price update trigger any liquidations?
    pub(crate) fn price_would_trigger(
        &self,
        store: &dyn Storage,
        new_price: PriceBaseInQuote,
    ) -> Result<bool> {
        // Get the latest price available in the oracle
        let oracle_price = match &self.config.spot_price {
            SpotPriceConfig::Manual { .. } => self.current_spot_price(store)?.price_notional,
            SpotPriceConfig::Oracle {
                feeds, feeds_usd, ..
            } => {
                let oracle_price = self.get_oracle_price(false)?;
                let market_id = self.market_id(store)?;
                let price_storage =
                    oracle_price.compose_price(market_id, feeds, feeds_usd, self.now())?;
                price_storage.price
            }
        };

        let new_price = new_price.into_notional_price(self.market_type(store)?);
        Ok(
            self.newly_liquidatable_position(store, oracle_price, new_price)
                || self.limit_order_newly_triggered_order(store, oracle_price, new_price),
        )
    }

    // this always executes the requested cranks
    // if there is no work to be done, then crank_exec itself will be cheap
    // QueryMsg::CrankStats can be used by clients to get heuristics and decide how many to crank
    pub fn crank_exec_batch(
        &self,
        ctx: &mut StateContext,
        start_n_execs_and_rewards: Option<(u32, Addr)>,
    ) -> Result<()> {
        const REGULAR_WEIGHT: u32 = 5;
        const COMPLETED_WEIGHT: u32 = 1;

        let mut crank_progress = if let Some((start_n_execs, rewards)) = start_n_execs_and_rewards {
            CrankProgress {
                weight_budget_left: start_n_execs * REGULAR_WEIGHT,
                paying_work_done: 0,
                actual: vec![],
                requested: start_n_execs,
                rewards,
            }
        } else {
            CRANK_BATCH_WEIGHT_LEFT.load(ctx.storage)?
        };

        loop {
            let price_point = match self.next_crank_timestamp(ctx.storage)? {
                None => break,
                Some(price_point) => price_point,
            };
            let work_info = self.crank_work(ctx.storage, price_point)?;

            let item_weight = match work_info {
                CrankWorkInfo::Completed { .. } => COMPLETED_WEIGHT,
                _ => REGULAR_WEIGHT,
            };

            match crank_progress.weight_budget_left.checked_sub(item_weight) {
                None => {
                    CRANK_BATCH_WEIGHT_LEFT.remove(ctx.storage);
                    break;
                }
                Some(weight_budget_left) => {
                    crank_progress.weight_budget_left = weight_budget_left;
                    crank_progress.actual.push((work_info.clone(), price_point));
                    if work_info.receives_crank_rewards() {
                        crank_progress.paying_work_done += 1;
                    }
                    if self.crank_exec(ctx, work_info, &price_point)? {
                        CRANK_BATCH_WEIGHT_LEFT.save(ctx.storage, &crank_progress)?;
                        return Ok(());
                    }
                }
            };
        }

        self.allocate_crank_fees(
            ctx,
            &crank_progress.rewards,
            crank_progress.paying_work_done,
        )?;
        ctx.response_mut().add_event(CrankExecBatchEvent {
            requested: crank_progress.requested as u64,
            paying: crank_progress.paying_work_done as u64,
            actual: crank_progress.actual,
        });

        Ok(())
    }

    /// Perform a single crank execution.
    fn crank_exec(
        &self,
        ctx: &mut StateContext,
        work_info: CrankWorkInfo,
        price_point: &PricePoint,
    ) -> Result<bool> {
        ctx.response_mut().add_event(CrankWorkInfoEvent {
            work_info: work_info.clone(),
            price_point: *price_point,
        });

        // do the work
        match work_info {
            CrankWorkInfo::ResetLpBalances {} => {
                self.crank_reset_lp_balances(ctx)?;
            }
            CrankWorkInfo::Liquifunding { position } => {
                let pos = get_position(ctx.storage, position)?;
                let starts_at = pos.liquifunded_at;
                let ends_at = pos.next_liquifunding;
                debug_assert!(ends_at <= price_point.timestamp);
                self.position_liquifund_store(
                    ctx,
                    pos,
                    starts_at,
                    ends_at,
                    true,
                    PositionSaveReason::Crank,
                )?;
            }
            CrankWorkInfo::CloseAllPositions { position } => {
                let pos = get_position(ctx.storage, position)?;
                self.close_position_via_msg(ctx, pos, *price_point)?;
            }
            CrankWorkInfo::Liquidation {
                position,
                liquidation_reason,
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
                        capped_exposure: Signed::zero(),
                        additional_losses: Collateral::zero(),
                        settlement_price: *price_point,
                        reason: PositionCloseReason::Liquidated(liquidation_reason),
                        closed_during_liquifunding: false,
                    },
                    MaybeClosedPosition::Close(x) => x,
                };
                self.close_position(ctx, close_position_instructions)?;
            }
            CrankWorkInfo::DeferredExec {
                deferred_exec_id,
                target: _,
            } => {
                self.process_deferred_exec(ctx, deferred_exec_id, price_point)?;
                return Ok(true);
            }
            CrankWorkInfo::LimitOrder { order_id } => {
                self.limit_order_execute_order(ctx, order_id, price_point)?;
            }
            CrankWorkInfo::Completed {} => {
                // Now that we've finished all updates for this price point,
                // accumulate the updated borrow fee and funding rates.
                self.accumulate_borrow_fee_rate(ctx, price_point)
                    .map_err(|e| anyhow::anyhow!("accumulate_borrow_fee_rate failed: {e:?}"))?;
                self.accumulate_funding_rate(ctx, price_point)?;
                LAST_CRANK_COMPLETED.save(ctx.storage, &price_point.timestamp)?;
            }
        }

        Ok(false)
    }
}
