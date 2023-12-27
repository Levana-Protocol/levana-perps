use crate::state::State;
use anyhow::Result;
use cosmwasm_std::{Addr, Storage};
use cw_storage_plus::{Item, Map};
use msg::contracts::market::deferred_execution::{
    DeferredExecExecutedEvent, DeferredExecFailure, DeferredExecId, DeferredExecItem,
    DeferredExecQueuedEvent, DeferredExecStatus, DeferredExecWithStatus, ListDeferredExecsResp,
};
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

    pub(crate) fn defer_execution(
        &self,
        ctx: &mut StateContext,
        trader: Addr,
        item: DeferredExecItem,
    ) -> Result<()> {
        // Owner check first
        if let Some(pos_id) = item.position_id() {
            self.position_assert_owner(
                ctx.storage,
                super::position::PositionOrId::Id(pos_id),
                &trader,
            )?;
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

        let position_id = item.position_id();

        DEFERRED_EXECS.save(
            ctx.storage,
            new_id,
            &DeferredExecWithStatus {
                created: self.now(),
                status: msg::contracts::market::deferred_execution::DeferredExecStatus::Pending,
                item,
                owner: trader.clone(),
            },
        )?;

        ctx.response_mut().add_event(DeferredExecQueuedEvent {
            deferred_exec_id: new_id,
            position_id,
            owner: trader,
        });

        Ok(())
    }

    pub(crate) fn next_crankable_deferred_exec_id(
        &self,
        store: &dyn Storage,
        price_point_timestamp: Timestamp,
        publish_time_base: Option<Timestamp>,
        publish_time_collateral: Option<Timestamp>,
    ) -> Result<Option<(DeferredExecId, Option<PositionId>)>> {
        let (id, item) = match self.get_next_deferred_execution(store)? {
            None => return Ok(None),
            Some(pair) => pair,
        };

        // Get the earliest of the free price timestamps. Motivation: if someone
        // publishes an old price from Pyth, we want to look at Pyth's time, not the block
        // time. This isn't theoretical: every case of an off-chain oracle timestamp should
        // be older than block time, and for on-chain oracles the timestamp of update
        // should never be newer than the block time.
        let mut publish_time = price_point_timestamp;
        if let Some(publish_time_base) = publish_time_base {
            debug_assert!(publish_time <= price_point_timestamp);
            publish_time = publish_time.min(publish_time_base);
        }
        if let Some(publish_time_collateral) = publish_time_collateral {
            debug_assert!(publish_time <= price_point_timestamp);
            publish_time = publish_time.min(publish_time_collateral);
        }

        Ok(if item.created <= publish_time {
            Some((id, item.item.position_id()))
        } else {
            None
        })
    }

    /// For sanity checks, get the total amount deposited pending deferred exec
    ///
    /// Note that this should _not_ ever be called on-chain, as it has O(n) complexity.
    pub(crate) fn deferred_exec_deposit_balance(&self, store: &dyn Storage) -> Result<Collateral> {
        let mut deposited = Collateral::zero();
        for res in DEFERRED_EXECS.range(store, None, None, Order::Descending) {
            let (_id, item) = res?;
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

        // Question on implementation: do we want to do the execution in the same message and be very careful about error handling, _or_ should we use a submessage and then check its error value?

        let mut item = DEFERRED_EXECS
            .may_load(ctx.storage, id)?
            .expect("process_deferred_exec: ID not found");
        item.status = DeferredExecStatus::Failure {
            reason: DeferredExecFailure::NotYetImplemented,
        };
        DEFERRED_EXECS.save(ctx.storage, id, &item)?;

        if let Some(amount) = NonZero::new(item.item.deposited_amount()) {
            self.add_token_transfer_msg(ctx, &item.owner, amount)?;
        }

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

        ctx.response_mut().add_event(DeferredExecExecutedEvent {
            deferred_exec_id: id,
            position_id: item.item.position_id(),
            owner: item.owner,
            success: false,
            desc: "Not yet implemented".to_owned(),
        });
        Ok(())
    }
}
