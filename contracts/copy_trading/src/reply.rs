use anyhow::bail;
use cosmwasm_std::{Reply, SubMsgResponse};
use perpswap::contracts::{copy_trading, market::deferred_execution::DeferredExecId};

use crate::{
    common::{get_current_processed_dec_queue_id, get_current_processed_inc_queue_id},
    prelude::*,
    types::State,
};

pub(crate) const REPLY_ID_OPEN_POSITION: u64 = 0;
pub(crate) const REPLY_ID_ADD_COLLATERAL_IMPACT_LEVERAGE: u64 = 1;
pub(crate) const REPLY_ID_ADD_COLLATERAL_IMPACT_SIZE: u64 = 2;
pub(crate) const REPLY_ID_REMOVE_COLLATERAL_IMPACT_LEVERAGE: u64 = 3;
pub(crate) const REPLY_ID_REMOVE_COLLATERAL_IMPACT_SIZE: u64 = 4;
pub(crate) const REPLY_ID_UPDATE_POSITION_LEVERAGE: u64 = 5;
pub(crate) const REPLY_ID_UPDATE_POSITION_TAKE_PROFIT_PRICE: u64 = 6;
pub(crate) const REPLY_ID_UPDATE_POSITION_STOP_LOSS_PRICE: u64 = 7;
pub(crate) const REPLY_ID_PLACE_LIMIT_ORDER: u64 = 8;
pub(crate) const REPLY_ID_CANCEL_LIMIT_ORDER: u64 = 9;
pub(crate) const REPLY_ID_CLOSE_POSITION: u64 = 10;

fn handle_sucess(storage: &mut dyn Storage, msg: SubMsgResponse, event: Event) -> Result<Event> {
    let deferred_exec_id: DeferredExecId = msg
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
    let exec_id = crate::state::REPLY_DEFERRED_EXEC_ID
        .may_load(storage)?
        .flatten();
    if exec_id.is_some() {
        bail!("Impossible: Deferred exec id already initialized")
    }
    crate::state::REPLY_DEFERRED_EXEC_ID.save(storage, &Some(deferred_exec_id))?;
    Ok(event.add_attribute("success", true.to_string()))
}

fn handle_dec_failure(
    storage: &mut dyn Storage,
    event: Event,
    error: String,
    handler: fn(&mut dyn Storage, Token, Box<DecMarketItem>) -> Result<()>,
) -> Result<Event> {
    // Updating position has failed
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
    handler(storage, token, item)?;
    queue_item.status = copy_trading::ProcessingStatus::Failed(FailedReason::MarketError {
        market_id,
        message: error,
    });
    crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &queue_id, &queue_item)?;
    crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &queue_id)?;
    let event = event.add_attribute("failure", true.to_string());
    Ok(event)
}

fn handle_inc_failure(storage: &mut dyn Storage, event: Event, error: String) -> Result<Event> {
    // Updating position has failed
    let queue_item = get_current_processed_inc_queue_id(storage)?;
    let (queue_id, mut queue_item) = match queue_item {
        Some(queue_item) => queue_item,
        None => bail!("Impossible: Work handle not able to find queue item"),
    };

    assert!(queue_item.status.in_progress());
    let (market_id, _, _) = match queue_item.item.clone() {
        IncQueueItem::MarketItem { id, token, item } => (id, token, item),
        _ => bail!("Impossible: Deferred work handler got non market item"),
    };
    queue_item.status = copy_trading::ProcessingStatus::Failed(FailedReason::MarketError {
        market_id,
        message: error,
    });
    crate::state::COLLATERAL_INCREASE_QUEUE.save(storage, &queue_id, &queue_item)?;
    crate::state::LAST_PROCESSED_INC_QUEUE_ID.save(storage, &queue_id)?;
    let event = event.add_attribute("failure", true.to_string());
    Ok(event)
}

fn dec_failure(storage: &mut dyn Storage, event: Event, error: String) -> Result<Event> {
    // Updating position has failed
    let queue_item = get_current_processed_dec_queue_id(storage)?;
    let (queue_id, mut queue_item) = match queue_item {
        Some(queue_item) => queue_item,
        None => bail!("Impossible: Work handle not able to find queue item"),
    };

    assert!(queue_item.status.in_progress());
    let (market_id, _, _) = match queue_item.item.clone() {
        DecQueueItem::MarketItem { id, token, item } => (id, token, item),
        _ => bail!("Impossible: Deferred work handler got non market item"),
    };
    queue_item.status = copy_trading::ProcessingStatus::Failed(FailedReason::MarketError {
        market_id,
        message: error,
    });
    crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &queue_id, &queue_item)?;
    crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &queue_id)?;
    let event = event.add_attribute("failure", true.to_string());
    Ok(event)
}

fn open_position_failure(
    storage: &mut dyn Storage,
    token: Token,
    item: Box<DecMarketItem>,
) -> Result<()> {
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
    Ok(())
}

fn place_limit_order_failure(
    storage: &mut dyn Storage,
    token: Token,
    item: Box<DecMarketItem>,
) -> Result<()> {
    let mut totals = crate::state::TOTALS
        .may_load(storage, &token)?
        .context("TOTALS store is empty")?;
    match *item {
        DecMarketItem::PlaceLimitOrder { collateral, .. } => {
            totals.collateral = totals.collateral.checked_add(collateral.raw())?;
            crate::state::TOTALS.save(storage, &token, &totals)?;
        }
        err => {
            bail!("Impossible: Reply handler got non place limit order: {err:?}")
        }
    }
    Ok(())
}

fn add_collateral_impact_leverage_failure(
    storage: &mut dyn Storage,
    token: Token,
    item: Box<DecMarketItem>,
) -> Result<()> {
    let mut totals = crate::state::TOTALS
        .may_load(storage, &token)?
        .context("TOTALS store is empty")?;
    match *item {
        DecMarketItem::UpdatePositionAddCollateralImpactLeverage { collateral, .. } => {
            totals.collateral = totals.collateral.checked_add(collateral.raw())?;
            crate::state::TOTALS.save(storage, &token, &totals)?;
        }
        err => {
            bail!("Impossible: Reply handler got non update position: {err:?}")
        }
    }
    Ok(())
}

fn add_collateral_impact_size_failure(
    storage: &mut dyn Storage,
    token: Token,
    item: Box<DecMarketItem>,
) -> Result<()> {
    let mut totals = crate::state::TOTALS
        .may_load(storage, &token)?
        .context("TOTALS store is empty")?;
    match *item {
        DecMarketItem::UpdatePositionAddCollateralImpactSize { collateral, .. } => {
            totals.collateral = totals.collateral.checked_add(collateral.raw())?;
            crate::state::TOTALS.save(storage, &token, &totals)?;
        }
        err => {
            bail!("Impossible: Reply handler got non update position: {err:?}")
        }
    }
    Ok(())
}

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let (_state, storage) = State::load_mut(deps, &env)?;
    let event = Event::new("reply").add_attribute("id", msg.id.to_string());
    let event = match msg.id {
        REPLY_ID_OPEN_POSITION => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(msg) => handle_sucess(storage, msg, event)?,
            cosmwasm_std::SubMsgResult::Err(error) => {
                handle_dec_failure(storage, event, error, open_position_failure)?
            }
        },
        REPLY_ID_ADD_COLLATERAL_IMPACT_LEVERAGE => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(msg) => handle_sucess(storage, msg, event)?,
            cosmwasm_std::SubMsgResult::Err(error) => handle_dec_failure(
                storage,
                event,
                error,
                add_collateral_impact_leverage_failure,
            )?,
        },
        REPLY_ID_ADD_COLLATERAL_IMPACT_SIZE => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(msg) => handle_sucess(storage, msg, event)?,
            cosmwasm_std::SubMsgResult::Err(error) => {
                handle_dec_failure(storage, event, error, add_collateral_impact_size_failure)?
            }
        },
        REPLY_ID_REMOVE_COLLATERAL_IMPACT_LEVERAGE
        | REPLY_ID_REMOVE_COLLATERAL_IMPACT_SIZE
        | REPLY_ID_CANCEL_LIMIT_ORDER
        | REPLY_ID_CLOSE_POSITION => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(msg) => handle_sucess(storage, msg, event)?,
            cosmwasm_std::SubMsgResult::Err(error) => handle_inc_failure(storage, event, error)?,
        },
        REPLY_ID_UPDATE_POSITION_LEVERAGE
        | REPLY_ID_UPDATE_POSITION_TAKE_PROFIT_PRICE
        | REPLY_ID_UPDATE_POSITION_STOP_LOSS_PRICE => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(msg) => handle_sucess(storage, msg, event)?,
            cosmwasm_std::SubMsgResult::Err(error) => dec_failure(storage, event, error)?,
        },
        REPLY_ID_PLACE_LIMIT_ORDER => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(msg) => handle_sucess(storage, msg, event)?,
            cosmwasm_std::SubMsgResult::Err(error) => {
                handle_dec_failure(storage, event, error, place_limit_order_failure)?
            }
        },
        _ => bail!("Got unknown reply id {}", msg.id),
    };
    Ok(Response::new().add_event(event))
}
