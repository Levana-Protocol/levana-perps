use crate::{prelude::*, types::State};

pub(crate) fn get_work(state: &State, storage: &dyn Storage) -> Result<WorkResp> {
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
            // Initially there would be no collateral at all. There is no
            // point trying to compute lp token value if that's the case.
            let totals = crate::state::TOTALS
                .may_load(storage, &token)
                .context("Could not load TOTALS")?
                .unwrap_or_default();
            if totals.shares == LpToken::zero() {
                return Ok(WorkResp::HasWork {
                    work_description: WorkDescription::ProcessQueueItem {
                        id: next_queue_position,
                    },
                });
            }

            let lp_token_value = crate::state::LP_TOKEN_VALUE.key(&token).may_load(storage)?;
            if let Some(lp_token_value) = lp_token_value {
                if lp_token_value.status.valid() {
                    return Ok(WorkResp::HasWork {
                        work_description: WorkDescription::ProcessQueueItem {
                            id: next_queue_position,
                        },
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
