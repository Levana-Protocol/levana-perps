use anyhow::bail;
use cosmwasm_std::{SubMsg, WasmMsg};
use perpswap::contracts::{
    copy_trading,
    market::deferred_execution::{DeferredExecId, GetDeferredExecResp},
};

use crate::{
    common::{
        get_current_processed_dec_queue_id, get_current_processed_inc_queue_id,
        SIX_HOURS_IN_SECONDS,
    },
    prelude::*,
    reply::{
        REPLY_ID_ADD_COLLATERAL_IMPACT_LEVERAGE, REPLY_ID_ADD_COLLATERAL_IMPACT_SIZE,
        REPLY_ID_CANCEL_LIMIT_ORDER, REPLY_ID_CLOSE_POSITION, REPLY_ID_OPEN_POSITION,
        REPLY_ID_PLACE_LIMIT_ORDER, REPLY_ID_REMOVE_COLLATERAL_IMPACT_LEVERAGE,
        REPLY_ID_REMOVE_COLLATERAL_IMPACT_SIZE, REPLY_ID_UPDATE_POSITION_LEVERAGE,
        REPLY_ID_UPDATE_POSITION_STOP_LOSS_PRICE, REPLY_ID_UPDATE_POSITION_TAKE_PROFIT_PRICE,
    },
    types::{
        DecQueueCrankResponse, DecQueuePosition, DecQueueResponse, IncQueueResponse, State,
        WalletInfo,
    },
};
use perpswap::contracts::market::entry::ExecuteMsg as MarketExecuteMsg;

fn get_dec_deferred_work(
    storage: &dyn Storage,
    state: &State,
    deferred_exec_id: DeferredExecId,
) -> Result<WorkResp> {
    let queue_item = get_current_processed_dec_queue_id(storage)?;
    let (_queue_id, queue_item) = match queue_item {
        Some((queue_id, queue_item)) => (queue_id, queue_item),
        None => bail!("Impossible: Work handle not able to find queue item"),
    };
    let market_id = match queue_item.item.clone() {
        DecQueueItem::MarketItem { id, .. } => id,
        _ => bail!("Impossible: Deferred work handler got non market item"),
    };
    let market_addr = crate::state::MARKETS
        .may_load(storage, &market_id)?
        .context("MARKETS state is empty")?
        .addr;
    let response = state.get_deferred_exec(&market_addr, deferred_exec_id)?;
    let status = match response {
        GetDeferredExecResp::Found { item } => item,
        GetDeferredExecResp::NotFound {} => {
            bail!("Impossible: Deferred exec id not found")
        }
    };
    if status.status.is_pending() {
        return Ok(WorkResp::NoWork);
    }
    Ok(WorkResp::HasWork {
        work_description: WorkDescription::HandleDeferredExecId {},
    })
}

fn get_inc_deferred_work(
    storage: &dyn Storage,
    state: &State,
    deferred_exec_id: DeferredExecId,
) -> Result<WorkResp> {
    let queue_item = get_current_processed_inc_queue_id(storage)?;
    let (_queue_id, queue_item) = match queue_item {
        Some((queue_id, queue_item)) => (queue_id, queue_item),
        None => bail!("Impossible: Inc Work handle not able to find queue item"),
    };
    let market_id = match queue_item.item.clone() {
        IncQueueItem::MarketItem { id, .. } => id,
        _ => bail!("Impossible: Deferred Inc work handler got non market item"),
    };
    let market_addr = crate::state::MARKETS
        .may_load(storage, &market_id)?
        .context("MARKETS state is empty")?
        .addr;
    let response = state.get_deferred_exec(&market_addr, deferred_exec_id)?;
    let status = match response {
        GetDeferredExecResp::Found { item } => item,
        GetDeferredExecResp::NotFound {} => {
            bail!("Impossible: Deferred exec id not found")
        }
    };
    if status.status.is_pending() {
        return Ok(WorkResp::NoWork);
    }
    Ok(WorkResp::HasWork {
        work_description: WorkDescription::HandleDeferredExecId {},
    })
}

fn get_work_from_dec_queue(
    queue_id: DecQueuePositionId,
    queue_item: DecQueuePosition,
    storage: &dyn Storage,
    state: &State,
) -> Result<WorkResp> {
    let queue_id = QueuePositionId::DecQueuePositionId(queue_id);
    let status = queue_item.status;
    let queue_item = queue_item.item;
    let requires_token = queue_item.requires_token();
    match requires_token {
        RequiresToken::Token { token } => {
            let lp_token_value = crate::state::LP_TOKEN_VALUE.key(&token).may_load(storage)?;
            match lp_token_value {
                Some(lp_token_value) => {
                    if lp_token_value.status.valid(&queue_id) {
                        if status == ProcessingStatus::InProgress {
                            let deferred_exec_id = crate::state::REPLY_DEFERRED_EXEC_ID
                                .may_load(storage)?
                                .flatten();
                            if let Some(deferred_exec_id) = deferred_exec_id {
                                return get_dec_deferred_work(storage, state, deferred_exec_id);
                            }
                        }
                        return Ok(WorkResp::HasWork {
                            work_description: WorkDescription::ProcessQueueItem { id: queue_id },
                        });
                    } else {
                        // LP token is invalid. But if we have
                        // deferred exec id, best to handle it
                        // before we try to compute lp token value.
                        let deferred_exec_id = crate::state::REPLY_DEFERRED_EXEC_ID
                            .may_load(storage)?
                            .flatten();
                        if let Some(deferred_exec_id) = deferred_exec_id {
                            return get_dec_deferred_work(storage, state, deferred_exec_id);
                        }
                    }
                }
                None => {
                    // For this token, the value was never in the store.
                    return check_balance_work(storage, state, &token);
                }
            }
            let market_infos = state.load_market_ids_with_token(storage, &token, None)?;

            for market in market_infos {
                let deferred_execs = state.load_deferred_execs(&market.addr, None, Some(1))?;
                let is_pending = deferred_execs
                    .items
                    .iter()
                    .any(|item| item.status.is_pending());
                if is_pending {
                    return Ok(WorkResp::NoWork);
                }

                let work = crate::state::MARKET_WORK_INFO
                    .may_load(storage, &market.id)?
                    .unwrap_or_default();

                // The status cannot be a batch operation.
                assert!(!work.processing_status.is_batch_operation());

                if work.processing_status.reset_required() {
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ResetStats { token },
                    });
                }
            }
            // We have gone through all the markets here and looks
            // like all the market has been validated. The only part
            // remaining to be done here is computation of lp token
            // value.
            check_balance_work(storage, state, &token)
        }
        RequiresToken::NoToken {} => {
            if status == ProcessingStatus::InProgress {
                let deferred_exec_id = crate::state::REPLY_DEFERRED_EXEC_ID
                    .may_load(storage)?
                    .flatten();
                if let Some(deferred_exec_id) = deferred_exec_id {
                    return get_dec_deferred_work(storage, state, deferred_exec_id);
                }
            }
            Ok(WorkResp::HasWork {
                work_description: WorkDescription::ProcessQueueItem { id: queue_id },
            })
        }
    }
}

fn get_batch_work(storage: &dyn Storage) -> Result<WorkResp> {
    let batch = crate::state::CURRENT_BATCH_WORK.may_load(storage)?;
    match batch {
        Some(batch) => match batch {
            crate::types::BatchWork::NoWork => Ok(WorkResp::NoWork),
            crate::types::BatchWork::BatchRebalance {
                start_from,
                balance,
                token,
            } => Ok(WorkResp::HasWork {
                work_description: WorkDescription::Rebalance {
                    token,
                    amount: balance,
                    start_from,
                },
            }),
            crate::types::BatchWork::BatchLpTokenValue {
                process_start_from,
                token,
                validate_start_from,
            } => Ok(WorkResp::HasWork {
                work_description: WorkDescription::ComputeLpTokenValue {
                    token,
                    process_start_from,
                    validate_start_from,
                },
            }),
        },
        None => Ok(WorkResp::NoWork),
    }
}

pub(crate) fn get_work(state: &State, storage: &dyn Storage) -> Result<WorkResp> {
    let batch_work = get_batch_work(storage)?;
    if batch_work.has_work() {
        return Ok(batch_work);
    }
    let market_status = crate::state::MARKET_LOADER_STATUS.may_load(storage)?;
    match market_status {
        Some(market_status) => match market_status {
            crate::types::MarketLoaderStatus::NotStarted => {
                return Ok(WorkResp::HasWork {
                    work_description: WorkDescription::LoadMarket {},
                })
            }
            crate::types::MarketLoaderStatus::OnGoing { .. } => {
                return Ok(WorkResp::HasWork {
                    work_description: WorkDescription::LoadMarket {},
                })
            }
            crate::types::MarketLoaderStatus::Finished { .. } => {
                let now = state.env.block.time;
                let last_seen = crate::state::LAST_MARKET_ADD_CHECK.may_load(storage)?;
                match last_seen {
                    Some(last_seen) => {
                        if last_seen.plus_seconds(SIX_HOURS_IN_SECONDS) < now.into() {
                            return Ok(WorkResp::HasWork {
                                work_description: WorkDescription::LoadMarket {},
                            });
                        }
                    }
                    None => bail!(
                        "Impossible: LAST_MARKET_ADD_CHECK uninitialized during Finished status"
                    ),
                }
            }
        },
        None => {
            return Ok(WorkResp::HasWork {
                work_description: WorkDescription::LoadMarket {},
            })
        }
    }
    let inc_queue_item = get_current_processed_inc_queue_id(storage)?;
    let (next_inc_queue_position, queue_item) = match inc_queue_item {
        Some((queue_id, queue_item)) => (queue_id, queue_item),
        None => {
            let dec_queue = get_current_processed_dec_queue_id(storage)?;
            match dec_queue {
                Some((queue_id, queue_item)) => {
                    let work = get_work_from_dec_queue(queue_id, queue_item, storage, state)?;
                    return Ok(work);
                }
                None => return Ok(WorkResp::NoWork),
            }
        }
    };

    let status = queue_item.status;
    let queue_item = queue_item.item;
    let requires_token = queue_item.requires_token();
    match requires_token {
        RequiresToken::Token { token } => {
            let lp_token_value = crate::state::LP_TOKEN_VALUE.key(&token).may_load(storage)?;
            match lp_token_value {
                Some(lp_token_value) => {
                    if lp_token_value
                        .status
                        .valid(&QueuePositionId::IncQueuePositionId(
                            next_inc_queue_position,
                        ))
                    {
                        if status == ProcessingStatus::InProgress {
                            let deferred_exec_id = crate::state::REPLY_DEFERRED_EXEC_ID
                                .may_load(storage)?
                                .flatten();
                            if let Some(deferred_exec_id) = deferred_exec_id {
                                return get_inc_deferred_work(storage, state, deferred_exec_id);
                            }
                        }
                        return Ok(WorkResp::HasWork {
                            work_description: WorkDescription::ProcessQueueItem {
                                id: QueuePositionId::IncQueuePositionId(next_inc_queue_position),
                            },
                        });
                    } else {
                        // LP token is invalid. But if we have
                        // deferred exec id, best to handle it
                        // before we try to compute lp token value.
                        let deferred_exec_id = crate::state::REPLY_DEFERRED_EXEC_ID
                            .may_load(storage)?
                            .flatten();
                        if let Some(deferred_exec_id) = deferred_exec_id {
                            return get_inc_deferred_work(storage, state, deferred_exec_id);
                        }
                        // We do not compute LP token here since we
                        // want to do some market pre-checks. These
                        // pre-checks are done down below.
                    }
                }
                None => {
                    // For this token, the value was never in the store.
                    return check_balance_work(storage, state, &token);
                }
            }
            let market_infos = state.load_market_ids_with_token(storage, &token, None)?;

            for market in market_infos {
                let deferred_execs = state.load_deferred_execs(&market.addr, None, Some(1))?;
                let is_pending = deferred_execs
                    .items
                    .iter()
                    .any(|item| item.status.is_pending());
                if is_pending {
                    return Ok(WorkResp::NoWork);
                }
                let work = crate::state::MARKET_WORK_INFO
                    .may_load(storage, &market.id)?
                    .unwrap_or_default();

                // The status cannot be a batch operation.
                assert!(!work.processing_status.is_batch_operation());

                if work.processing_status.reset_required() {
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ResetStats { token },
                    });
                }
            }
            // We have gone through all the markets here and looks
            // like all the market has been validated. The only part
            // remaining to be done here is computation of lp token
            // value.
            check_balance_work(storage, state, &token)
        }
        RequiresToken::NoToken {} => {
            if status == ProcessingStatus::InProgress {
                let deferred_exec_id = crate::state::REPLY_DEFERRED_EXEC_ID
                    .may_load(storage)?
                    .flatten();
                if let Some(deferred_exec_id) = deferred_exec_id {
                    return get_inc_deferred_work(storage, state, deferred_exec_id);
                }
            }
            Ok(WorkResp::HasWork {
                work_description: WorkDescription::ProcessQueueItem {
                    id: QueuePositionId::IncQueuePositionId(next_inc_queue_position),
                },
            })
        }
    }
}

pub(crate) fn process_queue_item(
    queue_pos_id: QueuePositionId,
    storage: &mut dyn Storage,
    state: &State,
    response: Response,
) -> Result<Response> {
    match queue_pos_id {
        QueuePositionId::IncQueuePositionId(queue_pos_id) => {
            let mut queue_item = crate::state::COLLATERAL_INCREASE_QUEUE
                .may_load(storage, &queue_pos_id)?
                .context("PENDING_QUEUE_ITEMS load failed")?;
            match queue_item.item.clone() {
                IncQueueItem::MarketItem {
                    id: market_id,
                    item,
                    token,
                } => {
                    match *item {
                        IncMarketItem::ClosePosition {
                            id,
                            slippage_assert,
                        } => {
                            let market_info = crate::state::MARKETS
                                .may_load(storage, &market_id)?
                                .context("MARKETS store is empty")?;
                            let msg = MarketExecuteMsg::ClosePosition {
                                id,
                                slippage_assert,
                            };
                            let msg = WasmMsg::Execute {
                                contract_addr: market_info.addr.to_string(),
                                msg: to_json_binary(&msg)?,
                                funds: vec![],
                            };
                            // We use reply always so that we also handle the error case
                            let sub_msg = SubMsg::reply_always(msg, REPLY_ID_CLOSE_POSITION);
                            let event =
                                Event::new("close-position").add_attribute("id", id.to_string());
                            let response = IncQueueResponse {
                                sub_msg,
                                collateral: None,
                                token,
                                event,
                                queue_item,
                                queue_id: queue_pos_id,
                            };
                            process_inc_queue(response, storage)
                        }
                        IncMarketItem::CancelLimitOrder { order_id } => {
                            let market_info = crate::state::MARKETS
                                .may_load(storage, &market_id)?
                                .context("MARKETS store is empty")?;
                            let msg = MarketExecuteMsg::CancelLimitOrder { order_id };
                            let msg = WasmMsg::Execute {
                                contract_addr: market_info.addr.to_string(),
                                msg: to_json_binary(&msg)?,
                                funds: vec![],
                            };
                            let sub_msg = SubMsg::reply_always(msg, REPLY_ID_CANCEL_LIMIT_ORDER);
                            let event = Event::new("cancel-limit-order")
                                .add_attribute("order-id", order_id.to_string());
                            let response = IncQueueResponse {
                                sub_msg,
                                collateral: None,
                                token,
                                event,
                                queue_item,
                                queue_id: queue_pos_id,
                            };
                            process_inc_queue(response, storage)
                        }
                        IncMarketItem::UpdatePositionRemoveCollateralImpactSize {
                            id,
                            amount,
                            slippage_assert,
                        } => {
                            let market_info = crate::state::MARKETS
                                .may_load(storage, &market_id)?
                                .context("MARKETS store is empty")?;
                            let crank_fee = state.estimate_crank_fee(&market_info)?;
                            let msg = market_info.token.into_market_execute_msg(
                                &market_info.addr,
                                crank_fee,
                                MarketExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                                    id,
                                    amount,
                                    slippage_assert,
                                },
                            )?;
                            // We use reply always so that we also handle the error case
                            let sub_msg =
                                SubMsg::reply_always(msg, REPLY_ID_REMOVE_COLLATERAL_IMPACT_SIZE);
                            let event = Event::new("update-position-remove-collateral-impact-size")
                                .add_attribute("amount", amount.to_string())
                                .add_attribute("maret-id", market_info.id.to_string())
                                .add_attribute("position-id", id.to_string());
                            let response = IncQueueResponse {
                                sub_msg,
                                collateral: Some(crank_fee),
                                token,
                                event,
                                queue_item,
                                queue_id: queue_pos_id,
                            };
                            process_inc_queue(response, storage)
                        }
                        IncMarketItem::UpdatePositionRemoveCollateralImpactLeverage {
                            id,
                            amount,
                        } => {
                            let market_info = crate::state::MARKETS
                                .may_load(storage, &market_id)?
                                .context("MARKETS store is empty")?;
                            let crank_fee = state.estimate_crank_fee(&market_info)?;
                            let msg = market_info.token.into_market_execute_msg(
                                &market_info.addr,
                                crank_fee,
                                MarketExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage {
                                    id,
                                    amount,
                                },
                            )?;
                            // We use reply always so that we also handle the error case
                            let sub_msg = SubMsg::reply_always(
                                msg,
                                REPLY_ID_REMOVE_COLLATERAL_IMPACT_LEVERAGE,
                            );
                            let event =
                                Event::new("update-position-remove-collateral-impact-leverage")
                                    .add_attribute("amount", amount.to_string())
                                    .add_attribute("maret-id", market_info.id.to_string())
                                    .add_attribute("position-id", id.to_string());
                            let response = IncQueueResponse {
                                sub_msg,
                                collateral: Some(crank_fee),
                                token,
                                event,
                                queue_item,
                                queue_id: queue_pos_id,
                            };
                            process_inc_queue(response, storage)
                        }
                    }
                }
                IncQueueItem::Deposit { funds, token } => {
                    let token_value = state.load_lp_token_value(storage, &token)?;
                    {
                        let mut totals = crate::state::TOTALS
                            .may_load(storage, &token)
                            .context("Could not load TOTALS")?
                            .unwrap_or_default();
                        totals.add_collateral(funds, &token_value)?;
                        crate::state::TOTALS.save(storage, &token, &totals)?;
                    }
                    {
                        let mut pending_deposits = crate::state::PENDING_DEPOSITS
                            .may_load(storage, &token)?
                            .unwrap_or_default();
                        pending_deposits = pending_deposits.checked_sub(funds.raw())?;
                        crate::state::PENDING_DEPOSITS.save(storage, &token, &pending_deposits)?;
                    }
                    let new_shares = {
                        let wallet_info = WalletInfo {
                            token,
                            wallet: queue_item.wallet.clone(),
                        };
                        let new_shares = token_value.collateral_to_shares(funds)?;
                        let shares = crate::state::SHARES.may_load(storage, &wallet_info)?;
                        let new_shares = match shares {
                            Some(shares) => shares.checked_add(new_shares.raw())?,
                            None => new_shares,
                        };
                        crate::state::SHARES.save(storage, &wallet_info, &new_shares)?;
                        new_shares
                    };
                    queue_item.status = copy_trading::ProcessingStatus::Finished;
                    crate::state::LAST_PROCESSED_INC_QUEUE_ID.save(storage, &queue_pos_id)?;
                    crate::state::COLLATERAL_INCREASE_QUEUE.save(
                        storage,
                        &queue_pos_id,
                        &queue_item,
                    )?;
                    let event = Event::new("deposit")
                        .add_attribute("funds", funds.to_string())
                        .add_attribute("shares", new_shares.to_string());
                    let response = response.add_event(event);
                    Ok(response)
                }
            }
        }
        QueuePositionId::DecQueuePositionId(queue_pos_id) => {
            let mut queue_item = crate::state::COLLATERAL_DECREASE_QUEUE
                .may_load(storage, &queue_pos_id)?
                .context("COLLATERAL_DECREASE_QUEUE load failed")?;
            match queue_item.item.clone() {
                DecQueueItem::Withdrawal { tokens, token } => {
                    crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &queue_pos_id)?;
                    let shares = tokens;
                    let wallet_info = WalletInfo {
                        token: token.clone(),
                        wallet: queue_item.wallet.clone(),
                    };
                    let mut event = Event::new("withdraw")
                        .add_attribute("wallet", wallet_info.wallet.to_string())
                        .add_attribute("token", token.to_string())
                        .add_attribute("shares", tokens.to_string());

                    let actual_shares = crate::state::SHARES.may_load(storage, &wallet_info)?;
                    // This is not a sanity check. This can happen
                    // whem multipe withdrawal requests are issues
                    // without the queue getting processed.
                    let actual_shares = match actual_shares {
                        Some(actual_shares) => {
                            if shares > actual_shares && shares != actual_shares {
                                queue_item.status =
                                    ProcessingStatus::Failed(FailedReason::NotEnoughShares {
                                        available: actual_shares.raw(),
                                        requested: shares.raw(),
                                    });
                                event = event.add_attribute("failed", true.to_string());
                                crate::state::COLLATERAL_DECREASE_QUEUE.save(
                                    storage,
                                    &queue_pos_id,
                                    &queue_item,
                                )?;
                                let response = Response::new().add_event(event);
                                return Ok(response);
                            }
                            actual_shares
                        }
                        None => {
                            queue_item.status =
                                ProcessingStatus::Failed(FailedReason::NotEnoughShares {
                                    available: LpToken::zero(),
                                    requested: shares.raw(),
                                });
                            event = event.add_attribute("failed", true.to_string());
                            crate::state::COLLATERAL_DECREASE_QUEUE.save(
                                storage,
                                &queue_pos_id,
                                &queue_item,
                            )?;
                            let response = Response::new().add_event(event);
                            return Ok(response);
                        }
                    };
                    let token_value = state.load_lp_token_value(storage, &wallet_info.token)?;
                    let funds = token_value.shares_to_collateral(shares)?;
                    let token = state.get_first_full_token_info(storage, &wallet_info.token)?;
                    let withdraw_msg = token.into_transfer_msg(&wallet_info.wallet, funds)?;
                    let remaining_shares = actual_shares.raw().checked_sub(shares.raw())?;
                    let contract_token = state.to_token(&token)?;
                    let mut totals = crate::state::TOTALS
                        .may_load(storage, &contract_token)?
                        .context("TOTALS is empty")?;
                    if funds.raw() > totals.collateral {
                        queue_item.status = copy_trading::ProcessingStatus::Failed(
                            FailedReason::NotEnoughCollateral {
                                available: totals.collateral,
                                requested: funds,
                            },
                        );
                        crate::state::COLLATERAL_DECREASE_QUEUE.save(
                            storage,
                            &queue_pos_id,
                            &queue_item,
                        )?;
                        let event = event
                            .add_attribute("failed", true.to_string())
                            .add_attribute("reason", "not-enough-collateral");
                        let response = Response::new().add_event(event);
                        return Ok(response);
                    } else {
                        totals.collateral = totals.collateral.checked_sub(funds.raw())?;
                        totals.shares = totals.shares.checked_sub(shares.raw())?;
                    };
                    let mut pending_store_update = || -> std::result::Result<(), _> {
                        if remaining_shares.is_zero() {
                            crate::state::SHARES.remove(storage, &wallet_info);
                        } else {
                            let remaining_shares = NonZero::new(remaining_shares)
                                .context("remaining_shares is zero")?;
                            crate::state::SHARES.save(storage, &wallet_info, &remaining_shares)?;
                        }
                        crate::state::TOTALS.save(storage, &contract_token, &totals)?;
                        let result: Result<()> = Ok(());
                        result
                    };
                    event = event.add_attribute("funds", funds.to_string());
                    let withdraw_msg = match withdraw_msg {
                        Some(withdraw_msg) => {
                            pending_store_update()?;
                            queue_item.status = copy_trading::ProcessingStatus::Finished;
                            Some(withdraw_msg)
                        }
                        None => {
                            // Collateral amount is less than chain's minimum representation.
                            // So, we do nothing. We just move on to the next item in the queue.
                            event = event
                                .add_attribute("funds-less-min-chain", true.to_string())
                                .add_attribute("failed", true.to_string());
                            queue_item.status = copy_trading::ProcessingStatus::Failed(
                                FailedReason::FundLessThanMinChain { funds },
                            );
                            None
                        }
                    };
                    let response = response.add_event(event);
                    let response = match withdraw_msg {
                        Some(withdraw_msg) => response.add_message(withdraw_msg),
                        None => response,
                    };
                    crate::state::COLLATERAL_DECREASE_QUEUE.save(
                        storage,
                        &queue_pos_id,
                        &queue_item,
                    )?;
                    Ok(response)
                }
                DecQueueItem::MarketItem {
                    id: market_id,
                    token,
                    item,
                } => match *item {
                    DecMarketItem::PlaceLimitOrder {
                        collateral,
                        trigger_price,
                        leverage,
                        direction,
                        stop_loss_override,
                        take_profit,
                    } => {
                        let market_info = crate::state::MARKETS
                            .may_load(storage, &market_id)?
                            .context("MARKETS store is empty")?;
                        let msg = market_info.token.into_market_execute_msg(
                            &market_info.addr,
                            collateral.raw(),
                            MarketExecuteMsg::PlaceLimitOrder {
                                trigger_price,
                                leverage,
                                direction,
                                stop_loss_override,
                                take_profit,
                            },
                        )?;
                        // We use reply always so that we also handle the error case
                        let sub_msg = SubMsg::reply_always(msg, REPLY_ID_PLACE_LIMIT_ORDER);
                        let event = Event::new("place-limit-order")
                            .add_attribute("direction", direction.as_str())
                            .add_attribute("leverage", leverage.to_string())
                            .add_attribute("collateral", collateral.to_string())
                            .add_attribute("trigger-price", trigger_price.to_string())
                            .add_attribute("take-profit", take_profit.to_string())
                            .add_attribute("market", market_info.id.as_str());
                        let response = DecQueueResponse {
                            sub_msg,
                            collateral,
                            token,
                            event,
                            queue_item,
                            queue_id: queue_pos_id,
                            response,
                        };
                        process_dec_queue(response, storage)
                    }
                    DecMarketItem::UpdatePositionStopLossPrice { id, stop_loss } => {
                        let market_info = crate::state::MARKETS
                            .may_load(storage, &market_id)?
                            .context("MARKETS store is empty")?;
                        let crank_fees = state.estimate_crank_fee(&market_info)?;
                        let msg = market_info.token.into_market_execute_msg(
                            &market_info.addr,
                            crank_fees,
                            MarketExecuteMsg::UpdatePositionStopLossPrice { id, stop_loss },
                        )?;
                        // We use reply always so that we also handle the error case
                        let sub_msg =
                            SubMsg::reply_always(msg, REPLY_ID_UPDATE_POSITION_STOP_LOSS_PRICE);
                        let event = Event::new("update-position-stop-loss-price")
                            .add_attribute("market-id", market_info.id.to_string())
                            .add_attribute("position-id", id.to_string());
                        let response = DecQueueCrankResponse {
                            sub_msg,
                            crank_fees,
                            token,
                            event,
                            queue_item,
                            queue_id: queue_pos_id,
                            response,
                        };
                        process_dec_crank_queue(response, storage)
                    }
                    DecMarketItem::UpdatePositionTakeProfitPrice { id, price } => {
                        let market_info = crate::state::MARKETS
                            .may_load(storage, &market_id)?
                            .context("MARKETS store is empty")?;
                        let crank_fees = state.estimate_crank_fee(&market_info)?;
                        let msg = market_info.token.into_market_execute_msg(
                            &market_info.addr,
                            crank_fees,
                            MarketExecuteMsg::UpdatePositionTakeProfitPrice { id, price },
                        )?;
                        // We use reply always so that we also handle the error case
                        let sub_msg =
                            SubMsg::reply_always(msg, REPLY_ID_UPDATE_POSITION_TAKE_PROFIT_PRICE);
                        let event = Event::new("update-position-take-profit-price")
                            .add_attribute("market-id", market_info.id.to_string())
                            .add_attribute("price", price.to_string())
                            .add_attribute("position-id", id.to_string());
                        let response = DecQueueCrankResponse {
                            sub_msg,
                            crank_fees,
                            token,
                            event,
                            queue_item,
                            queue_id: queue_pos_id,
                            response,
                        };
                        process_dec_crank_queue(response, storage)
                    }
                    DecMarketItem::UpdatePositionLeverage {
                        id,
                        leverage,
                        slippage_assert,
                    } => {
                        let market_info = crate::state::MARKETS
                            .may_load(storage, &market_id)?
                            .context("MARKETS store is empty")?;
                        let crank_fees = state.estimate_crank_fee(&market_info)?;
                        let msg = market_info.token.into_market_execute_msg(
                            &market_info.addr,
                            crank_fees,
                            MarketExecuteMsg::UpdatePositionLeverage {
                                id,
                                leverage,
                                slippage_assert,
                            },
                        )?;
                        // We use reply always so that we also handle the error case
                        let sub_msg = SubMsg::reply_always(msg, REPLY_ID_UPDATE_POSITION_LEVERAGE);
                        let event = Event::new("update-position-leverage")
                            .add_attribute("market-id", market_info.id.to_string())
                            .add_attribute("position-id", id.to_string());
                        let response = DecQueueCrankResponse {
                            sub_msg,
                            crank_fees,
                            token,
                            event,
                            queue_item,
                            queue_id: queue_pos_id,
                            response,
                        };
                        process_dec_crank_queue(response, storage)
                    }
                    DecMarketItem::UpdatePositionAddCollateralImpactSize {
                        collateral,
                        id,
                        slippage_assert,
                    } => {
                        let market_info = crate::state::MARKETS
                            .may_load(storage, &market_id)?
                            .context("MARKETS store is empty")?;
                        let msg = market_info.token.into_market_execute_msg(
                            &market_info.addr,
                            collateral.raw(),
                            MarketExecuteMsg::UpdatePositionAddCollateralImpactSize {
                                id,
                                slippage_assert,
                            },
                        )?;
                        // We use reply always so that we also handle the error case
                        let sub_msg =
                            SubMsg::reply_always(msg, REPLY_ID_ADD_COLLATERAL_IMPACT_SIZE);
                        let event = Event::new("update-position-add-collateral-impact-size")
                            .add_attribute("collateral", collateral.to_string())
                            .add_attribute("market-id", market_info.id.to_string())
                            .add_attribute("position-id", id.to_string());
                        let response = DecQueueResponse {
                            sub_msg,
                            collateral,
                            token,
                            event,
                            queue_item,
                            queue_id: queue_pos_id,
                            response,
                        };
                        process_dec_queue(response, storage)
                    }
                    DecMarketItem::UpdatePositionAddCollateralImpactLeverage { collateral, id } => {
                        let market_info = crate::state::MARKETS
                            .may_load(storage, &market_id)?
                            .context("MARKETS store is empty")?;
                        let msg = market_info.token.into_market_execute_msg(
                            &market_info.addr,
                            collateral.raw(),
                            MarketExecuteMsg::UpdatePositionAddCollateralImpactLeverage { id },
                        )?;
                        // We use reply always so that we also handle the error case
                        let sub_msg =
                            SubMsg::reply_always(msg, REPLY_ID_ADD_COLLATERAL_IMPACT_LEVERAGE);
                        let event = Event::new("update-position-add-collateral-impact-leverage")
                            .add_attribute("collateral", collateral.to_string())
                            .add_attribute("market-id", market_info.id.to_string())
                            .add_attribute("position-id", id.to_string());
                        let response = DecQueueResponse {
                            sub_msg,
                            collateral,
                            token,
                            event,
                            queue_item,
                            queue_id: queue_pos_id,
                            response,
                        };
                        process_dec_queue(response, storage)
                    }
                    DecMarketItem::OpenPosition {
                        slippage_assert,
                        leverage,
                        direction,
                        stop_loss_override,
                        take_profit,
                        collateral,
                    } => {
                        let id = crate::state::MARKETS
                            .may_load(storage, &market_id)?
                            .context("MARKETS store is empty")?;
                        let msg = id.token.into_market_execute_msg(
                            &id.addr,
                            collateral.raw(),
                            MarketExecuteMsg::OpenPosition {
                                slippage_assert,
                                leverage,
                                direction,
                                stop_loss_override,
                                take_profit,
                            },
                        )?;
                        // We use reply always so that we also handle the error case
                        let sub_msg = SubMsg::reply_always(msg, REPLY_ID_OPEN_POSITION);
                        let event = Event::new("open-position")
                            .add_attribute("direction", direction.as_str())
                            .add_attribute("leverage", leverage.to_string())
                            .add_attribute("collateral", collateral.to_string())
                            .add_attribute("market", id.id.as_str());
                        let response = DecQueueResponse {
                            sub_msg,
                            collateral,
                            token,
                            event,
                            queue_item,
                            queue_id: queue_pos_id,
                            response,
                        };
                        process_dec_queue(response, storage)
                    }
                },
            }
        }
    }
}

pub fn check_balance_work(storage: &dyn Storage, state: &State, token: &Token) -> Result<WorkResp> {
    let market_token = state.get_first_full_token_info(storage, token)?;
    let contract_balance = market_token.query_balance(&state.querier, &state.my_addr)?;
    let totals = crate::state::TOTALS
        .may_load(storage, token)?
        .unwrap_or_default();
    let pending_deposits = crate::state::PENDING_DEPOSITS
        .may_load(storage, token)?
        .unwrap_or_default();
    let leader_comission = crate::state::LEADER_COMMISSION
        .may_load(storage, token)?
        .unwrap_or_default();
    let total = totals
        .collateral
        .checked_add(pending_deposits)?
        .checked_add(leader_comission.unclaimed)?;
    let diff = total.diff(contract_balance);
    // This diff number was chosen arbitrarly by opening and closing
    // 1500 positions which resulted in some leader commission.
    let is_approximate_same = diff <= "0.00001".parse().unwrap();
    if is_approximate_same {
        // We know for a fact that this is a fresh computation of LP
        // token value, so we need to Reset old stats if present.
        let market_infos = state.load_market_ids_with_token(storage, token, None)?;
        for market_info in market_infos {
            let work = crate::state::MARKET_WORK_INFO
                .may_load(storage, &market_info.id)?
                .unwrap_or_default();
            assert!(!work.processing_status.is_batch_operation());
            if !work.processing_status.not_started_yet() {
                // This means it is either Validated or Reset required status
                return Ok(WorkResp::HasWork {
                    work_description: WorkDescription::ResetStats {
                        token: token.clone(),
                    },
                });
            }
        }
        Ok(WorkResp::HasWork {
            work_description: WorkDescription::ComputeLpTokenValue {
                token: token.clone(),
                process_start_from: None,
                validate_start_from: None,
            },
        })
    } else {
        // Now there are multiple reasons why it would be
        // unbalanced. Multiple positions could have been liquidated
        // or someone just sent money to this contract.
        let rebalance_amount = contract_balance.checked_sub(total)?;
        let rebalance_amount =
            NonZero::new(rebalance_amount).context("Impossible: rebalance_amount is zero")?;
        Ok(WorkResp::HasWork {
            work_description: WorkDescription::Rebalance {
                token: token.clone(),
                amount: rebalance_amount,
                start_from: None,
            },
        })
    }
}

fn process_dec_queue(response: DecQueueResponse, storage: &mut dyn Storage) -> Result<Response> {
    let mut totals = crate::state::TOTALS
        .may_load(storage, &response.token)?
        .context("TOTALS store is empty")?;
    let mut queue_item = response.queue_item;
    let token = response.token;
    if totals.collateral >= response.collateral.raw() {
        totals.collateral = totals.collateral.checked_sub(response.collateral.raw())?;
    } else {
        let event = response.event.add_attribute("failure", true.to_string());
        queue_item.status = ProcessingStatus::Failed(FailedReason::NotEnoughCollateral {
            available: totals.collateral,
            requested: response.collateral,
        });
        crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &response.queue_id, &queue_item)?;
        crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &response.queue_id)?;
        return Ok(response.response.add_event(event));
    }
    crate::state::TOTALS.save(storage, &token, &totals)?;
    let mut token_value = crate::state::LP_TOKEN_VALUE
        .may_load(storage, &token)?
        .context("LP_TOKEN_VALUE store is empty")?;
    token_value.set_outdated();
    crate::state::LP_TOKEN_VALUE.save(storage, &token, &token_value)?;
    queue_item.status = ProcessingStatus::InProgress;
    crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &response.queue_id, &queue_item)?;
    let result = response.response.add_event(response.event);
    let result = result.add_submessage(response.sub_msg);
    Ok(result)
}

fn process_dec_crank_queue(
    response: DecQueueCrankResponse,
    storage: &mut dyn Storage,
) -> Result<Response> {
    let mut totals = crate::state::TOTALS
        .may_load(storage, &response.token)?
        .context("TOTALS store is empty")?;
    let mut queue_item = response.queue_item;
    let token = response.token;
    if totals.collateral >= response.crank_fees {
        totals.collateral = totals.collateral.checked_sub(response.crank_fees)?;
    } else {
        let event = response.event.add_attribute("failure", true.to_string());
        queue_item.status = ProcessingStatus::Failed(FailedReason::NotEnoughCrankFee {
            available: totals.collateral,
            requested: response.crank_fees,
        });
        crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &response.queue_id, &queue_item)?;
        crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &response.queue_id)?;
        return Ok(response.response.add_event(event));
    }
    crate::state::TOTALS.save(storage, &token, &totals)?;
    let mut token_value = crate::state::LP_TOKEN_VALUE
        .may_load(storage, &token)?
        .context("LP_TOKEN_VALUE store is empty")?;
    token_value.set_outdated();
    crate::state::LP_TOKEN_VALUE.save(storage, &token, &token_value)?;
    queue_item.status = ProcessingStatus::InProgress;
    crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &response.queue_id, &queue_item)?;
    let result = response.response.add_event(response.event);
    let result = result.add_submessage(response.sub_msg);
    Ok(result)
}

fn process_inc_queue(response: IncQueueResponse, storage: &mut dyn Storage) -> Result<Response> {
    let mut queue_item = response.queue_item;
    let token = response.token;
    if let Some(collateral) = response.collateral {
        let mut totals = crate::state::TOTALS
            .may_load(storage, &token)?
            .context("TOTALS store is empty")?;
        if totals.collateral >= collateral {
            totals.collateral = totals.collateral.checked_sub(collateral)?;
            crate::state::TOTALS.save(storage, &token, &totals)?;
        } else {
            let event = response.event.add_attribute("failure", true.to_string());
            queue_item.status = ProcessingStatus::Failed(FailedReason::NotEnoughCrankFee {
                available: totals.collateral,
                requested: collateral,
            });
            crate::state::COLLATERAL_INCREASE_QUEUE.save(
                storage,
                &response.queue_id,
                &queue_item,
            )?;
            crate::state::LAST_PROCESSED_INC_QUEUE_ID.save(storage, &response.queue_id)?;
            return Ok(Response::new().add_event(event));
        }
    }
    let mut token_value = crate::state::LP_TOKEN_VALUE
        .may_load(storage, &token)?
        .context("LP_TOKEN_VALUE store is empty")?;
    token_value.set_outdated();
    crate::state::LP_TOKEN_VALUE.save(storage, &token, &token_value)?;
    queue_item.status = ProcessingStatus::InProgress;
    crate::state::COLLATERAL_INCREASE_QUEUE.save(storage, &response.queue_id, &queue_item)?;
    let result = Response::new().add_event(response.event);
    let result = result.add_submessage(response.sub_msg);
    Ok(result)
}
