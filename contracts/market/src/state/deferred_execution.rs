use crate::state::position::get_position;
use crate::state::State;
use anyhow::Result;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Empty, Storage, SubMsg, SubMsgResult};
use cw_storage_plus::{Item, Map};
use msg::contracts::market::deferred_execution::{
    DeferredExecCompleteTarget, DeferredExecExecutedEvent, DeferredExecId, DeferredExecItem,
    DeferredExecQueuedEvent, DeferredExecStatus, DeferredExecTarget, DeferredExecWithStatus,
    GetDeferredExecResp, ListDeferredExecsResp,
};
use msg::contracts::market::fees::events::TradeId;
use msg::contracts::market::order::OrderId;
use msg::contracts::market::position::PositionId;
use msg::prelude::*;

use super::StateContext;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct DeferredExecLatestIds {
    pub(crate) issued: DeferredExecId,
    pub(crate) processed: Option<DeferredExecId>,
}

impl DeferredExecLatestIds {
    pub(crate) fn queue_size(&self) -> u32 {
        u32::try_from(self.issued.u64() - self.processed.map_or(0, |x| x.u64())).unwrap_or(u32::MAX)
    }
}

/// Stores the last issued and [DeferredExecId]
const DEFERRED_EXEC_LATEST_IDS: Item<DeferredExecLatestIds> =
    Item::new(namespace::DEFERRED_EXEC_LATEST_IDS);

/// All deferred execution items with their status.
const DEFERRED_EXECS: Map<DeferredExecId, DeferredExecWithStatus> =
    Map::new(namespace::DEFERRED_EXECS);

/// Deferred exec IDs grouped by wallet.
const DEFERRED_EXECS_BY_WALLET: Map<(Addr, DeferredExecId), ()> =
    Map::new(namespace::DEFERRED_EXECS_BY_WALLET);

/// Pending deferred exec action for the given position.
const PENDING_DEFERRED_FOR_POSITION: Map<(PositionId, DeferredExecId), ()> =
    Map::new(namespace::PENDING_DEFERRED_FOR_POSITION);

/// Pending deferred exec action for the given order.
const PENDING_DEFERRED_FOR_ORDER: Map<(OrderId, DeferredExecId), ()> =
    Map::new(namespace::PENDING_DEFERRED_FOR_ORDER);

/// Is the limit order already scheduled to be canceled?
const IS_LIMIT_ORDER_CANCELING: Map<OrderId, ()> = Map::new(namespace::IS_LIMIT_ORDER_CANCELING);

/// Is the position already scheduled to be closed?
const IS_POSITION_CLOSING: Map<PositionId, ()> = Map::new(namespace::IS_POSITION_CLOSING);

impl State<'_> {
    pub(crate) fn deferred_execution_latest_ids(
        &self,
        store: &dyn Storage,
    ) -> Result<Option<DeferredExecLatestIds>> {
        DEFERRED_EXEC_LATEST_IDS
            .may_load(store)
            .map_err(|e| e.into())
    }

    pub(crate) fn get_next_deferred_execution(
        &self,
        store: &dyn Storage,
    ) -> Result<Option<(DeferredExecId, DeferredExecWithStatus)>> {
        let DeferredExecLatestIds { issued, processed } =
            match DEFERRED_EXEC_LATEST_IDS.may_load(store)? {
                Some(x) => x,
                None => return Ok(None),
            };
        let next_id = match processed {
            None => DeferredExecId::first(),
            Some(processed) => {
                debug_assert!(processed <= issued);

                if processed >= issued {
                    return Ok(None);
                } else {
                    processed.next()
                }
            }
        };

        debug_assert!(next_id <= issued);

        let item = DEFERRED_EXECS
            .may_load(store, next_id)?
            .expect("Logic error, next_id in get_next_deferred_execution does not exist");
        Ok(Some((next_id, item)))
    }

    pub(crate) fn list_deferred_execs(
        &self,
        store: &dyn Storage,
        addr: Addr,
        start_after: Option<DeferredExecId>,
        limit: Option<u32>,
    ) -> Result<ListDeferredExecsResp> {
        let mut iter = DEFERRED_EXECS_BY_WALLET.prefix(addr).range(
            store,
            None,
            start_after.map(Bound::exclusive),
            Order::Descending,
        );
        let limit = usize::try_from(limit.unwrap_or(10))
            .expect("list_deferred_execs: could not convert limit to usize")
            .min(30);
        let mut items = vec![];
        let mut last_id = None;
        for res in iter.by_ref().take(limit) {
            let (id, _) = res?;
            last_id = Some(id);
            let item = DEFERRED_EXECS.may_load(store, id)?.expect(
                "Logic error in list_deferred_execs: DEFERRED_EXECS.may_load returned None",
            );
            items.push(item);
        }
        let next_start_after = last_id.filter(|_| iter.next().is_some());
        Ok(ListDeferredExecsResp {
            items,
            next_start_after,
        })
    }

    pub(crate) fn get_deferred_exec(
        &self,
        store: &dyn Storage,
        id: DeferredExecId,
    ) -> Result<DeferredExecWithStatus> {
        DEFERRED_EXECS.load(store, id).map_err(|e| e.into())
    }

    pub(crate) fn get_deferred_exec_resp(
        &self,
        store: &dyn Storage,
        id: DeferredExecId,
    ) -> Result<GetDeferredExecResp> {
        Ok(match DEFERRED_EXECS.may_load(store, id)? {
            Some(item) => GetDeferredExecResp::Found {
                item: Box::new(item),
            },
            None => GetDeferredExecResp::NotFound {},
        })
    }

    pub(crate) fn defer_execution(
        &self,
        ctx: &mut StateContext,
        trader: Addr,
        mut item: DeferredExecItem,
        funds_attached: Result<NonZero<Collateral>>,
    ) -> Result<()> {
        // Calculate the next ID first so that we can figure out how many items are in the queue already.
        let (new_id, new_latest_ids, queue_size) =
            match DEFERRED_EXEC_LATEST_IDS.may_load(ctx.storage)? {
                None => {
                    let new_id = DeferredExecId::first();
                    let latest_ids = DeferredExecLatestIds {
                        issued: new_id,
                        processed: None,
                    };
                    (new_id, latest_ids, 0)
                }
                Some(mut latest_ids) => {
                    let queue_size =
                        latest_ids.issued.u64() - latest_ids.processed.map_or(0, |x| x.u64());
                    let new_id = latest_ids.issued.next();
                    latest_ids.issued = new_id;
                    (new_id, latest_ids, queue_size)
                }
            };
        DEFERRED_EXEC_LATEST_IDS.save(ctx.storage, &new_latest_ids)?;
        DEFERRED_EXECS_BY_WALLET.save(ctx.storage, (trader.clone(), new_id), &())?;

        // Determine the amount of crank fee we need to charge.
        let new_crank_fee_usd = self.config.crank_fee_charged
            + self
                .config
                .crank_fee_surcharge
                // Intentionally dividing at the u64 level and not Decimal so we
                // get the expected step-wise decrease from round-down divison.
                .checked_mul_dec(Decimal256::from_ratio(queue_size / 10, 1u32))?;
        // Even though we never want to use historical prices while executing
        // the deferred queue, for collecting the crank fee we have to use an
        // existing price in the system. This calculation isn't part of the
        // security of the platform, but rather is a convenience for charging
        // crank fees in USD instead of collateral. Using the most recent
        // price point is our best option.
        let current_price = self.current_spot_price(ctx.storage)?;
        let new_crank_fee = current_price.usd_to_collateral(new_crank_fee_usd);

        // Check the owner is correct and try to charge the crank fee
        let target = item.target();
        match &mut item {
            DeferredExecItem::OpenPosition {
                amount,
                crank_fee,
                crank_fee_usd,
                ..
            }
            | DeferredExecItem::PlaceLimitOrder {
                amount,
                crank_fee,
                crank_fee_usd,
                ..
            } => {
                debug_assert!(funds_attached.is_err());
                *crank_fee = new_crank_fee;
                *crank_fee_usd = new_crank_fee_usd;
                *amount = amount.checked_sub(new_crank_fee)?;
                self.collect_crank_fee(
                    ctx,
                    TradeId::Deferred(new_id),
                    new_crank_fee,
                    new_crank_fee_usd,
                )?;
            }
            DeferredExecItem::UpdatePositionAddCollateralImpactLeverage { id, amount }
            | DeferredExecItem::UpdatePositionAddCollateralImpactSize { id, amount, .. } => {
                // Take the crank fee from the submitted amount
                debug_assert!(funds_attached.is_err());
                *amount = amount.checked_sub(new_crank_fee)?;

                // Update the position to reflect the crank fee charged
                let mut pos = get_position(ctx.storage, *id)?;
                pos.crank_fee
                    .checked_add_assign(new_crank_fee, &current_price)?;
                pos.deposit_collateral
                    .checked_add_assign(new_crank_fee.into_signed(), &current_price)?;

                // Update the protocol to track the crank fee available in general fees
                self.collect_crank_fee(
                    ctx,
                    TradeId::Position(*id),
                    new_crank_fee,
                    new_crank_fee_usd,
                )?;
                self.position_save_no_recalc(ctx, &pos)?;
            }
            // For these five items, we have the trader send funds to cover a
            // crank fee. Any remaining funds are allocated to the LP rewards category for the
            // user to collect later.
            DeferredExecItem::UpdatePositionRemoveCollateralImpactLeverage { id, .. }
            | DeferredExecItem::UpdatePositionRemoveCollateralImpactSize { id, .. }
            | DeferredExecItem::UpdatePositionLeverage { id, .. }
            | DeferredExecItem::UpdatePositionMaxGains { id, .. }
            | DeferredExecItem::SetTriggerOrder { id, .. }
            // If the user provides a slippage assert, it's possible that the
            // close will fail. This could lead to a spam attack on the protocol if we don't
            // collect fees.
            | DeferredExecItem::ClosePosition {
                id,
                slippage_assert: Some(_),
            } => {
                // Take out the crank fee and put the rest of the funds into rewards to be collected.
                let funds_attached = if new_crank_fee.is_zero() {
                    Collateral::zero()
                } else {
                    funds_attached
                        .context("No funds provided for update crank fee")?
                        .raw()
                };
                let funds_after_crank = funds_attached.checked_sub(new_crank_fee)?;

                // Update the protocol to track the crank fee available in general fees
                self.collect_crank_fee(
                    ctx,
                    TradeId::Position(*id),
                    new_crank_fee,
                    new_crank_fee_usd,
                )?;

                // Give the remainder back to the user as rewards
                if let Some(funds_after_crank) = NonZero::new(funds_after_crank) {
                    self.return_funds_to_user(ctx, &trader, funds_after_crank, &current_price)?;
                }

                // Update the crank fee on charged on the position
                let mut pos = get_position(ctx.storage, *id)?;
                pos.crank_fee
                    .checked_add_assign(new_crank_fee, &current_price)?;
                pos.deposit_collateral
                    .checked_add_assign(new_crank_fee.into_signed(), &current_price)?;
                self.position_save_no_recalc(ctx, &pos)?;
            }
            DeferredExecItem::ClosePosition { id, slippage_assert: None } => {
                anyhow::ensure!(
                    funds_attached.is_err(),
                    "No funds should be attached for close position"
                );
                // We don't charge a separate crank fee for closing a position without a slippage assert,
                // but we ensure we only queue up such a work item once to avoid a spam
                // attack.
                if IS_POSITION_CLOSING.has(ctx.storage, *id) {
                    return Err(MarketError::PositionAlreadyClosing {
                        position_id: id.u64().into(),
                    }
                    .into_anyhow());
                }
                IS_POSITION_CLOSING.save(ctx.storage, *id, &())?;
            }
            DeferredExecItem::CancelLimitOrder { order_id } => {
                anyhow::ensure!(
                    funds_attached.is_err(),
                    "No funds should be attached for cancel limit order"
                );
                // We don't charge a separate crank fee for canceling a limit
                // order, but we ensure we only queue up such a work item once to avoid a spam
                // attack.
                if IS_LIMIT_ORDER_CANCELING.has(ctx.storage, *order_id) {
                    return Err(MarketError::LimitOrderAlreadyCanceling {
                        order_id: order_id.u64().into(),
                    }
                    .into_anyhow());
                }
                IS_LIMIT_ORDER_CANCELING.save(ctx.storage, *order_id, &())?;
            }
        }
        match target {
            DeferredExecTarget::DoesNotExist => {}
            DeferredExecTarget::Position(pos_id) => {
                self.position_assert_owner(ctx.storage, pos_id, &trader)?;
            }
            DeferredExecTarget::Order(order_id) => {
                self.limit_order_assert_owner(ctx.storage, &trader, order_id)?;
            }
        }

        DEFERRED_EXECS.save(
            ctx.storage,
            new_id,
            &DeferredExecWithStatus {
                id: new_id,
                created: self.now(),
                status: msg::contracts::market::deferred_execution::DeferredExecStatus::Pending,
                item,
                owner: trader.clone(),
            },
        )?;

        match target {
            DeferredExecTarget::DoesNotExist => (),
            DeferredExecTarget::Position(pos_id) => {
                PENDING_DEFERRED_FOR_POSITION.save(ctx.storage, (pos_id, new_id), &())?;
            }
            DeferredExecTarget::Order(order_id) => {
                PENDING_DEFERRED_FOR_ORDER.save(ctx.storage, (order_id, new_id), &())?;
            }
        }

        ctx.response_mut().add_event(DeferredExecQueuedEvent {
            deferred_exec_id: new_id,
            target,
            owner: trader,
        });

        Ok(())
    }

    pub(crate) fn next_crankable_deferred_exec_id(
        &self,
        store: &dyn Storage,
        publish_time: Timestamp,
    ) -> Result<Option<(DeferredExecId, DeferredExecTarget)>> {
        let (id, item) = match self.get_next_deferred_execution(store)? {
            None => return Ok(None),
            Some(pair) => pair,
        };

        Ok(if item.created < publish_time {
            Some((id, item.item.target()))
        } else {
            None
        })
    }

    /// For sanity checks, get the total amount deposited pending deferred exec
    ///
    /// Note that this should _not_ ever be called on-chain, as it has O(n) complexity.
    #[cfg(feature = "sanity")]
    pub(crate) fn deferred_exec_deposit_balance(&self, store: &dyn Storage) -> Result<Collateral> {
        let mut deposited = Collateral::zero();
        for res in DEFERRED_EXECS.range(store, None, None, Order::Descending) {
            let (id, item) = res?;
            anyhow::ensure!(id == item.id);
            if !item.status.is_pending() {
                break;
            }
            deposited += item.item.deposited_amount();
        }
        Ok(deposited)
    }

    pub(crate) fn process_deferred_exec(
        &self,
        ctx: &mut StateContext,
        id: DeferredExecId,
        price_point: &PricePoint,
    ) -> Result<()> {
        ctx.response_mut().add_raw_submessage(SubMsg::reply_always(
            CosmosMsg::<Empty>::Wasm(cosmwasm_std::WasmMsg::Execute {
                contract_addr: self.env.contract.address.clone().into_string(),
                msg: to_binary(&MarketExecuteMsg::PerformDeferredExec {
                    id,
                    price_point_timestamp: price_point.timestamp,
                })?,
                funds: vec![],
            }),
            // Let's use the deferred exec ID as the reply ID for now. In theory
            // we could have other things in the future that need to use a reply. But we can
            // always modify the code at that point to use a different mechanism.
            id.u64(),
        ));

        // We immediately update the data structure so that if we crank multiple
        // items we continue with the next ID.
        let DeferredExecLatestIds { issued, processed } = DEFERRED_EXEC_LATEST_IDS
            .may_load(ctx.storage)?
            .expect("Logic error: process_deferred_exec had no DEFERRED_EXEC_LATEST_IDS");
        debug_assert_eq!(
            processed.map_or_else(DeferredExecId::first, |x| x.next()),
            id
        );
        DEFERRED_EXEC_LATEST_IDS.save(
            ctx.storage,
            &DeferredExecLatestIds {
                issued,
                processed: Some(id),
            },
        )?;

        // Clear out the close position tracking so that, if closing a position
        // fails for slippage asserts, we can retry again later.
        if let DeferredExecItem::ClosePosition {
            id,
            slippage_assert: None,
        } = DEFERRED_EXECS.load(ctx.storage, id)?.item
        {
            IS_POSITION_CLOSING.remove(ctx.storage, id);
        }

        Ok(())
    }

    pub(crate) fn handle_deferred_exec_reply(
        &self,
        ctx: &mut StateContext,
        id: DeferredExecId,
        res: SubMsgResult,
    ) -> Result<()> {
        let mut item = DEFERRED_EXECS
            .may_load(ctx.storage, id)?
            .expect("handle_deferred_exec_reply: ID not found");

        let (success, desc) = match res {
            SubMsgResult::Ok(_) => match item.status {
                DeferredExecStatus::Success { .. } => (true, "Execution successful".to_owned()),
                DeferredExecStatus::Pending => {
                    anyhow::bail!("handle_deferred_exec_reply: success reply but still Pending")
                }
                DeferredExecStatus::Failure { .. } => {
                    anyhow::bail!("handle_deferred_exec_reply: success reply but see a Failure")
                }
            },
            SubMsgResult::Err(e) => {
                let price_point = self.next_crank_timestamp(ctx.storage)?;
                // Replace empty error from the submessage with validation error.
                let e = if let Some(price_point) = price_point {
                    self.deferred_validate(ctx.storage, id, &price_point)
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or(e)
                } else {
                    e
                };

                anyhow::ensure!(
                    item.status == DeferredExecStatus::Pending,
                    "Item should still be pending, but actual status is {:?}",
                    item.status
                );
                item.status = DeferredExecStatus::Failure {
                    reason: e.clone(),
                    executed: self.now(),
                    crank_price: price_point,
                };

                // It didn't work, so give them back their money
                if let Some(amount) = NonZero::new(item.item.deposited_amount()) {
                    self.add_token_transfer_msg(ctx, &item.owner, amount)?;
                }

                (false, e)
            }
        };
        DEFERRED_EXECS.save(ctx.storage, id, &item)?;

        let target = item.item.target();
        match target {
            DeferredExecTarget::DoesNotExist => (),
            DeferredExecTarget::Position(pos_id) => {
                // This is just a sanity check
                anyhow::ensure!(PENDING_DEFERRED_FOR_POSITION.has(ctx.storage, (pos_id, id)));

                PENDING_DEFERRED_FOR_POSITION.remove(ctx.storage, (pos_id, id));
            }
            DeferredExecTarget::Order(order_id) => {
                // This is just a sanity check
                anyhow::ensure!(PENDING_DEFERRED_FOR_ORDER.has(ctx.storage, (order_id, id)));

                PENDING_DEFERRED_FOR_ORDER.remove(ctx.storage, (order_id, id));
            }
        }

        ctx.response_mut().add_event(DeferredExecExecutedEvent {
            deferred_exec_id: id,
            target,
            owner: item.owner,
            success,
            desc,
        });
        Ok(())
    }

    pub(crate) fn load_deferred_exec_item(
        &self,
        store: &dyn Storage,
        id: DeferredExecId,
    ) -> Result<DeferredExecWithStatus> {
        DEFERRED_EXECS
            .may_load(store, id)?
            .with_context(|| format!("Could not load deferred exec item {id}"))
    }

    pub(crate) fn mark_deferred_exec_success(
        &self,
        ctx: &mut StateContext,
        mut item: DeferredExecWithStatus,
        target: DeferredExecCompleteTarget,
    ) -> Result<()> {
        item.status = DeferredExecStatus::Success {
            target,
            executed: self.now(),
        };
        DEFERRED_EXECS.save(ctx.storage, item.id, &item)?;
        Ok(())
    }

    pub(crate) fn assert_no_pending_deferred(
        &self,
        store: &dyn Storage,
        id: PositionId,
    ) -> Result<()> {
        if PENDING_DEFERRED_FOR_POSITION
            .prefix(id)
            .keys(store, None, None, Order::Ascending)
            .next()
            .is_some()
        {
            Err(MarketError::PendingDeferredExec {}.into_anyhow())
        } else {
            Ok(())
        }
    }
}
