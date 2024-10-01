use anyhow::bail;

use crate::{common::SIX_HOURS_IN_SECONDS, prelude::*, types::State};

pub(crate) fn get_work(state: &State, storage: &dyn Storage, env: &Env) -> Result<WorkResp> {
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
                let now = env.block.time;
                let last_seen = crate::state::LAST_MARKET_ADD_CHECK.may_load(storage)?;
                match last_seen {
                    Some(last_seen) => {
                        if last_seen.plus_seconds(SIX_HOURS_IN_SECONDS) > now.into() {
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

    let queue = crate::state::LAST_PROCESSED_QUEUE_ID.may_load(storage)?;
    let next_queue_position = match queue {
        Some(queue_position) => queue_position.next(),
        None => QueuePositionId::new(0),
    };
    let queue_item = crate::state::PENDING_QUEUE_ITEMS
        .key(&next_queue_position)
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
                                id: next_queue_position,
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
                id: next_queue_position,
            },
        }),
    }
}
