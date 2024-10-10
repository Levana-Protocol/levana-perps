use anyhow::bail;
use cosmwasm_std::{Reply, SubMsgResponse};
use perpswap::contracts::{copy_trading, market::deferred_execution::DeferredExecId};

use crate::{common::get_current_processed_dec_queue_id, prelude::*, types::State};

pub(crate) const REPLY_ID_OPEN_POSITION: u64 = 0;
pub(crate) const REPLY_ID_ADD_COLLATERAL_IMPACT_LEVERAGE: u64 = 1;

fn handle_sucess(storage: &mut dyn Storage, res: SubMsgResponse, event: Event) -> Result<Event> {
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
    Ok(event.add_attribute("success", true.to_string()))
}

fn open_position_handle_failure(
    storage: &mut dyn Storage,
    event: Event,
    error: String,
) -> Result<Event> {
    // Opening position has failed
    let queue_item = get_current_processed_dec_queue_id(storage)?;
    let (queue_id, mut queue_item) = match queue_item {
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
    queue_item.status = copy_trading::ProcessingStatus::Failed(FailedReason::MarketError {
        market_id,
        message: error,
    });
    crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &queue_id, &queue_item)?;
    crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &queue_id)?;
    let event = event.add_attribute("failure", true.to_string());
    Ok(event)
}

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let (_state, storage) = State::load_mut(deps, &env)?;
    let event = Event::new("reply").add_attribute("id", msg.id.to_string());
    let event = match msg.id {
        REPLY_ID_OPEN_POSITION => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(msg) => handle_sucess(storage, msg, event)?,
            cosmwasm_std::SubMsgResult::Err(error) => {
                open_position_handle_failure(storage, event, error)?
            }
        },
        _ => bail!("Got unknown reply id {}", msg.id),
    };
    Ok(Response::new().add_event(event))
}
