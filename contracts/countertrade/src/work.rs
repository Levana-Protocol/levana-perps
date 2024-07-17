use msg::contracts::market::entry::StatusResp;
use shared::storage::PricePoint;

use crate::prelude::*;

pub(crate) fn get_work_for(
    _storage: &dyn Storage,
    state: &State,
    market: &MarketInfo,
    totals: &Totals,
) -> Result<HasWorkResp> {
    if totals.collateral.is_zero() {
        return Ok(HasWorkResp::NoWork {});
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

    panic!("Cannot perform: {desc:#?}");

    Ok(Response::new())
}
