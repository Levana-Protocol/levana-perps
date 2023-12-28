use crate::state::State;
use anyhow::Result;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Empty, Storage, SubMsg, SubMsgResult};
use cw_storage_plus::{Item, Map};
use msg::contracts::market::deferred_execution::{
    DeferredExecCompleteTarget, DeferredExecExecutedEvent, DeferredExecId, DeferredExecItem,
    DeferredExecQueuedEvent, DeferredExecStatus, DeferredExecTarget, DeferredExecWithStatus,
    GetDeferredExecResp, ListDeferredExecsResp,
};
use msg::contracts::market::order::OrderId;
use msg::contracts::market::position::PositionId;
use msg::prelude::*;

use super::StateContext;

#[derive(serde::Serialize, serde::Deserialize)]
struct DeferredExecLatestIds {
    issued: DeferredExecId,
    processed: Option<DeferredExecId>,
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

impl State<'_> {
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
            .expect("list_deferred_execs: could not convert limit to usize");
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
        item: DeferredExecItem,
    ) -> Result<()> {
        // Owner check first
        let target = item.target();
        match target {
            DeferredExecTarget::DoesNotExist => (),
            DeferredExecTarget::Position(pos_id) => {
                self.position_assert_owner(ctx.storage, pos_id, &trader)?;
            }
            DeferredExecTarget::Order(order_id) => {
                self.limit_order_assert_owner(ctx.storage, &trader, order_id)?;
            }
        }

        let (new_id, new_latest_ids) = match DEFERRED_EXEC_LATEST_IDS.may_load(ctx.storage)? {
            None => {
                let new_id = DeferredExecId::first();
                let latest_ids = DeferredExecLatestIds {
                    issued: new_id,
                    processed: None,
                };
                (new_id, latest_ids)
            }
            Some(mut latest_ids) => {
                let new_id = latest_ids.issued.next();
                latest_ids.issued = new_id;
                (new_id, latest_ids)
            }
        };
        DEFERRED_EXEC_LATEST_IDS.save(ctx.storage, &new_latest_ids)?;
        DEFERRED_EXECS_BY_WALLET.save(ctx.storage, (trader.clone(), new_id), &())?;

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
    ) -> Result<()> {
        // For now we always fail, this obviously needs to be fixed.
        ctx.response_mut().add_raw_submessage(SubMsg::reply_always(
            CosmosMsg::<Empty>::Wasm(cosmwasm_std::WasmMsg::Execute {
                contract_addr: self.env.contract.address.clone().into_string(),
                msg: to_binary(&MarketExecuteMsg::PerformDeferredExec { id })?,
                funds: vec![],
            }),
            // Let's use the deferred exec ID as the reply ID for now. In theory
            // we could have other things in the future that need to use a reply. But we can
            // always modify the code at that point to use a different mechanism.
            id.u64(),
        ));

        // TODO need to deduct crank fees from either the new funds or the existing position. Can look at limit order logic.

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
                // Replace empty error from the submessage with validation error.
                let e = self
                    .deferred_validate(ctx, id)
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or(e);

                anyhow::ensure!(
                    item.status == DeferredExecStatus::Pending,
                    "Item should still be pending, but actual status is {:?}",
                    item.status
                );
                item.status = DeferredExecStatus::Failure {
                    reason: e.clone(),
                    executed: self.now(),
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
