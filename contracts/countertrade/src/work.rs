use msg::contracts::market::entry::StatusResp;

use crate::prelude::*;

pub(crate) fn get_work_for(
    _storage: &dyn Storage,
    state: &State,
    market: &MarketInfo,
) -> Result<HasWorkResp> {
    let status: StatusResp = state
        .querier
        .query_wasm_smart(&market.addr, &MarketQueryMsg::Status { price: None })
        .context("Unable to query market status")?;
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
