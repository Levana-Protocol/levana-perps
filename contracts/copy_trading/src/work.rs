use anyhow::bail;
use cosmwasm_std::CosmosMsg;

use crate::{
    prelude::*,
    types::{State, WalletInfo},
};

fn get_work_from_dec_queue(
    queue_id: DecQueuePositionId,
    storage: &dyn Storage,
    state: &State,
) -> Result<WorkResp> {
    let queue_item = crate::state::COLLATERAL_DECREASE_QUEUE
        .key(&queue_id)
        .may_load(storage)?;
    let queue_item = match queue_item {
        Some(queue_item) => queue_item.item,
        None => return Ok(WorkResp::NoWork),
    };
    let requires_token = queue_item.requires_token();
    match requires_token {
        RequiresToken::Token { token } => {
            let lp_token_value = crate::state::LP_TOKEN_VALUE.key(&token).may_load(storage)?;
            match lp_token_value {
                Some(lp_token_value) => {
                    if lp_token_value.status.valid() {
                        return Ok(WorkResp::HasWork {
                            work_description: WorkDescription::ProcessQueueItem {
                                id: QueuePositionId::DecQueuePositionId(queue_id),
                            },
                        });
                    }
                }
                None => {
                    // For this token, the value was never in the store.
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ComputeLpTokenValue { token },
                    });
                }
            }
            let market_works =
                crate::state::MARKET_WORK_INFO.range(storage, None, None, Order::Descending);
            for market_work in market_works {
                let (market_id, work) = market_work?;

                let market_info = crate::state::MARKETS.key(&market_id).load(storage)?;
                let market_token = state.to_token(&market_info.token)?;
                if market_token != token {
                    continue;
                }

                let deferred_execs = state.load_deferred_execs(&market_info.addr, None, Some(1))?;

                let is_pending = deferred_execs
                    .items
                    .iter()
                    .any(|item| item.status.is_pending());
                if is_pending {
                    return Ok(WorkResp::NoWork);
                }

                if work.processing_status.reset_required() {
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ResetStats {},
                    });
                }
                if !work.processing_status.is_validated() {
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ComputeLpTokenValue { token },
                    });
                }
            }
            // We have gone through all the markets here and looks
            // like all the market has been validated. The only part
            // remaining to be done here is computation of lp token
            // value.
            Ok(WorkResp::HasWork {
                work_description: WorkDescription::ComputeLpTokenValue { token },
            })
        }
        RequiresToken::NoToken {} => Ok(WorkResp::HasWork {
            work_description: WorkDescription::ProcessQueueItem {
                id: QueuePositionId::DecQueuePositionId(queue_id),
            },
        }),
    }
}

pub(crate) fn get_work(state: &State, storage: &dyn Storage) -> Result<WorkResp> {
    let inc_queue = crate::state::LAST_PROCESSED_INC_QUEUE_ID.may_load(storage)?;
    let dec_queue = crate::state::LAST_PROCESSED_DEC_QUEUE_ID.may_load(storage)?;
    let next_inc_queue_position = match inc_queue {
        Some(queue_position) => queue_position.next(),
        None => IncQueuePositionId::new(0),
    };
    let next_dec_queue_position = match dec_queue {
        Some(queue_position) => queue_position.next(),
        None => DecQueuePositionId::new(0),
    };
    let queue_item = crate::state::COLLATERAL_INCREASE_QUEUE
        .key(&next_inc_queue_position)
        .may_load(storage)?;

    let queue_item = match queue_item {
        Some(queue_item) => queue_item.item,
        None => {
            let work = get_work_from_dec_queue(next_dec_queue_position, storage, state)?;
            return Ok(work);
        }
    };

    let requires_token = queue_item.requires_token();

    match requires_token {
        RequiresToken::Token { token } => {
            let lp_token_value = crate::state::LP_TOKEN_VALUE.key(&token).may_load(storage)?;
            match lp_token_value {
                Some(lp_token_value) => {
                    if lp_token_value.status.valid() {
                        return Ok(WorkResp::HasWork {
                            work_description: WorkDescription::ProcessQueueItem {
                                id: QueuePositionId::IncQueuePositionId(next_inc_queue_position),
                            },
                        });
                    }
                }
                None => {
                    // For this token, the value was never in the store.
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ComputeLpTokenValue { token },
                    });
                }
            }

            let market_works =
                crate::state::MARKET_WORK_INFO.range(storage, None, None, Order::Descending);
            for market_work in market_works {
                let (market_id, work) = market_work?;

                let market_info = crate::state::MARKETS.key(&market_id).load(storage)?;
                let market_token = state.to_token(&market_info.token)?;
                if market_token != token {
                    continue;
                }

                let deferred_execs = state.load_deferred_execs(&market_info.addr, None, Some(1))?;

                let is_pending = deferred_execs
                    .items
                    .iter()
                    .any(|item| item.status.is_pending());
                if is_pending {
                    return Ok(WorkResp::NoWork);
                }

                if work.processing_status.reset_required() {
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ResetStats {},
                    });
                }
                if !work.processing_status.is_validated() {
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ComputeLpTokenValue { token },
                    });
                }
            }
            // We have gone through all the markets here and looks
            // like all the market has been validated. The only part
            // remaining to be done here is computation of lp token
            // value.
            Ok(WorkResp::HasWork {
                work_description: WorkDescription::ComputeLpTokenValue { token },
            })
        }
        RequiresToken::NoToken {} => Ok(WorkResp::HasWork {
            work_description: WorkDescription::ProcessQueueItem {
                id: QueuePositionId::IncQueuePositionId(next_inc_queue_position),
            },
        }),
    }
}

pub(crate) fn process_queue_item(
    id: QueuePositionId,
    storage: &mut dyn Storage,
    state: &State,
) -> Result<(Event, Option<CosmosMsg>)> {
    match id {
        QueuePositionId::IncQueuePositionId(id) => {
            let queue_item = crate::state::COLLATERAL_INCREASE_QUEUE
                .may_load(storage, &id)?
                .context("PENDING_QUEUE_ITEMS load failed")?;
            match queue_item.item {
                IncQueueItem::Deposit { funds, token } => {
                    let mut totals = crate::state::TOTALS
                        .may_load(storage, &token)
                        .context("Could not load TOTALS")?
                        .unwrap_or_default();
                    let token_value = state.load_lp_token_value(storage, &token)?;
                    let new_shares = totals.add_collateral(funds, token_value)?;
                    crate::state::TOTALS.save(storage, &token, &totals)?;
                    let wallet_info = WalletInfo {
                        token,
                        wallet: queue_item.wallet,
                    };
                    let shares = crate::state::SHARES.may_load(storage, &wallet_info)?;
                    let new_shares = match shares {
                        Some(shares) => shares.checked_add(new_shares.raw())?,
                        None => new_shares,
                    };
                    crate::state::SHARES.save(storage, &wallet_info, &new_shares)?;
                    crate::state::LAST_PROCESSED_INC_QUEUE_ID.save(storage, &id)?;
                    let event = Event::new("deposit")
                        .add_attribute("funds", funds.to_string())
                        .add_attribute("shares", new_shares.to_string());
                    Ok((event, None))
                }
            }
        }
        QueuePositionId::DecQueuePositionId(id) => {
            let queue_item = crate::state::COLLATERAL_DECREASE_QUEUE
                .may_load(storage, &id)?
                .context("COLLATERAL_DECREASE_QUEUE load failed")?;
            match queue_item.item {
                DecQueueItem::Withdrawal { tokens, token } => {
                    let shares = tokens;
                    let wallet_info = WalletInfo {
                        token,
                        wallet: queue_item.wallet,
                    };
                    let actual_shares = crate::state::SHARES.may_load(storage, &wallet_info)?;
                    // This is a sanity check. This should never happen.
                    let actual_shares = match actual_shares {
                        Some(actual_shares) => {
                            if shares > actual_shares && shares != actual_shares {
                                bail!("Requesting more withdrawal than balance")
                            }
                            actual_shares
                        }
                        None => bail!("No shares found"),
                    };
                    let token_value = state.load_lp_token_value(storage, &wallet_info.token)?;
                    let funds = token_value.shares_to_collateral(shares)?;
                    let token = state.get_full_token_info(storage, &wallet_info.token)?;
                    let withdraw_msg = token.into_transfer_msg(&wallet_info.wallet, funds)?;

                    let remaining_shares = actual_shares.raw().checked_sub(shares.raw())?;
                    crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &id)?;
                    let contract_token = state.to_token(&token)?;
                    let mut totals = crate::state::TOTALS
                        .may_load(storage, &contract_token)?
                        .context("TOTALS is empty")?;
                    if funds.raw() > totals.collateral {
                        bail!("Not enough collateral")
                    } else {
                        totals.collateral = totals.collateral.checked_sub(funds.raw())?;
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
                    let mut event = Event::new("withdraw")
                        .add_attribute("wallet", wallet_info.wallet.to_string())
                        .add_attribute("funds", funds.to_string())
                        .add_attribute("burned-shares", shares.to_string());
                    let withdraw_msg = match withdraw_msg {
                        Some(withdraw_msg) => {
                            pending_store_update()?;
                            Some(withdraw_msg)
                        }
                        None => {
                            // Collateral amount is less than chain's minimum representation.
                            // So, we do nothing. We just move on to the next item in the queue.
                            event = event.add_attribute("funds-less-min-chain", true.to_string());
                            None
                        }
                    };
                    Ok((event, withdraw_msg))
                }
                DecQueueItem::OpenPosition {} => todo!(),
            }
        }
    }
}
