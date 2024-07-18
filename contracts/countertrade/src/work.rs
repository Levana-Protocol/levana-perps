use cosmwasm_std::{CosmosMsg, SubMsg};
use msg::contracts::market::entry::StatusResp;
use shared::storage::PricePoint;

use crate::prelude::*;

pub(crate) fn get_work_for(
    _storage: &dyn Storage,
    state: &State,
    market: &MarketInfo,
    totals: &Totals,
) -> Result<HasWorkResp> {
    if totals.shares.is_zero() {
        return Ok(HasWorkResp::NoWork {});
    }

    let pos = PositionsInfo::load(state, market)?;

    let pos = match pos {
        PositionsInfo::TooManyPositions { to_close } => {
            return Ok(HasWorkResp::Work {
                desc: WorkDescription::ClosePosition { pos_id: to_close },
            })
        }
        PositionsInfo::NoPositions => None,
        PositionsInfo::OnePosition { pos } => Some(pos),
    };

    if totals.collateral.is_zero() && pos.is_none() {
        return Ok(HasWorkResp::Work {
            desc: WorkDescription::ResetShares,
        });
    }

    let price: PricePoint = state
        .querier
        .query_wasm_smart(&market.addr, &MarketQueryMsg::SpotPrice { timestamp: None })
        .context("Unable to query market spot price")?;
    let status: StatusResp = state
        .querier
        .query_wasm_smart(&market.addr, &MarketQueryMsg::Status { price: None })
        .context("Unable to query market status")?;

    let collateral_in_usd = price.collateral_to_usd(totals.collateral);
    if collateral_in_usd < status.config.minimum_deposit_usd {
        return Ok(HasWorkResp::NoWork {});
    }

    if status.long_funding > state.config.max_funding.into_signed() {
        Ok(HasWorkResp::Work {
            desc: WorkDescription::GoShort,
        })
    } else if status.short_funding > state.config.max_funding.into_signed() {
        Ok(HasWorkResp::Work {
            desc: WorkDescription::GoLong,
        })
    } else {
        Ok(HasWorkResp::NoWork {})
    }
}

pub(crate) fn execute(
    storage: &mut dyn Storage,
    state: State,
    market: MarketInfo,
) -> Result<Response> {
    let totals = crate::state::TOTALS
        .may_load(storage, &market.id)?
        .unwrap_or_default();

    let work = get_work_for(storage, &state, &market, &totals)?;

    let desc = match work {
        HasWorkResp::NoWork {} => bail!("No work items available"),
        HasWorkResp::Work { desc } => desc,
    };

    let mut res = Response::new()
        .add_event(Event::new("work-desc").add_attribute("desc", format!("{desc:?}")));

    match &desc {
        WorkDescription::GoShort => todo!("go short"),
        WorkDescription::GoLong => todo!("go long"),
        WorkDescription::ClosePosition { pos_id } => {
            res = res.add_event(
                Event::new("close-position").add_attribute("position-id", pos_id.to_string()),
            );
            let msg = CosmosMsg::Wasm(cosmwasm_std::WasmMsg::Execute {
                contract_addr: market.addr.into_string(),
                msg: to_json_binary(&MarketExecuteMsg::ClosePosition {
                    id: *pos_id,
                    slippage_assert: None,
                })?,
                funds: vec![],
            });
            res = res.add_submessage(SubMsg::reply_on_success(msg, 0));
            debug_assert!(!crate::state::REPLY.exists(storage));
            let previous_balance = state.get_local_token_balance(&market.token)?;
            crate::state::REPLY.save(
                storage,
                &ReplyState::ClosingPositions {
                    market: market.id,
                    previous_balance,
                },
            )?;
        }
        WorkDescription::ResetShares => todo!(),
    }

    Ok(res)
}
