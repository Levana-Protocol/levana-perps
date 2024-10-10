use anyhow::bail;
use cosmwasm_std::Reply;
use perpswap::contracts::{copy_trading, market::deferred_execution::DeferredExecId};

use crate::{prelude::*, types::State};

pub(crate) const REPLY_ID_OPEN_POSITION: u64 = 0;
pub(crate) const REPLY_ID_ADD_COLLATERAL_IMPACT_LEVERAGE: u64 = 1;

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let (_state, storage) = State::load_mut(deps, &env)?;
    if msg.id == REPLY_ID_OPEN_POSITION {
        match msg.result {
            cosmwasm_std::SubMsgResult::Ok(res) => {
                let deferred_exec_id: DeferredExecId = res
                    .events
                    .iter()
                    .find(|e| e.ty == "wasm-deferred-exec-queued")
                    .context("No wasm-deferred-exec-queued event found")?
                    .attributes
                    .iter()
                    .find(|a| a.key == "deferred-exec-id")
                    .context("No deferred-exec-id found in wasm-deferred-exec-queued event")?
                    .value
                    .parse()?;
                crate::state::REPLY_DEFERRED_EXEC_ID.save(storage, &Some(deferred_exec_id))?;
            }
            cosmwasm_std::SubMsgResult::Err(e) => {
                // Opening position has failed
                let queue_id = crate::state::LAST_PROCESSED_DEC_QUEUE_ID.may_load(storage)?;
                let queue_id = match queue_id {
                    Some(queue_id) => queue_id.next(),
                    None => DecQueuePositionId::new(0),
                };
                let queue_item =
                    crate::state::COLLATERAL_DECREASE_QUEUE.may_load(storage, &queue_id)?;
                let mut queue_item = match queue_item {
                    Some(queue_item) => queue_item,
                    None => bail!("Impossible: Work handle not able to find queue item"),
                };
                assert!(queue_item.status.in_progress());
                let (market_id, token, item) = match queue_item.item.clone() {
                    DecQueueItem::MarketItem { id, token, item } => (id, token, item),
                    _ => bail!("Impossible: Deferred work handler got non market item"),
                };
                let mut totals = crate::state::TOTALS
                    .may_load(storage, &token)?
                    .context("TOTALS store is empty")?;
                match *item {
                    DecMarketItem::OpenPosition { collateral, .. } => {
                        totals.collateral = totals.collateral.checked_add(collateral.raw())?;
                        crate::state::TOTALS.save(storage, &token, &totals)?;
                    }
                    err => {
                        bail!("Impossible: Reply handler got non open position: {err:?}")
                    }
                }
                queue_item.status =
                    copy_trading::ProcessingStatus::Failed(FailedReason::MarketError {
                        market_id,
                        message: e,
                    });
                crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &queue_id, &queue_item)?;
                crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &queue_id)?;
                // todo: add events
            }
        };
    } else {
        bail!("Got unknown reply id {}", msg.id)
    }
    Ok(Response::new().add_event(Event::new("reply")))
}
