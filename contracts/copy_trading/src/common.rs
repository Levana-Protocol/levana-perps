use crate::{
    prelude::*,
    types::{MarketInfo, PositionInfo, State},
};
use anyhow::{Context, Result};
use msg::contracts::market::{
    entry::PositionsQueryFeeApproach,
    position::{PositionId, PositionsResp},
};

impl<'a> State<'a> {
    pub(crate) fn load(deps: Deps<'a>, env: Env) -> Result<(Self, &'a dyn Storage)> {
        let config = crate::state::CONFIG
            .load(deps.storage)
            .context("Could not load config")?;
        Ok((
            State {
                config,
                api: deps.api,
                querier: deps.querier,
                my_addr: env.contract.address,
            },
            deps.storage,
        ))
    }

    pub(crate) fn load_mut(deps: DepsMut<'a>, env: Env) -> Result<(Self, &'a mut dyn Storage)> {
        let config = crate::state::CONFIG
            .load(deps.storage)
            .context("Could not load config")?;
        Ok((
            State {
                config,
                api: deps.api,
                querier: deps.querier,
                my_addr: env.contract.address,
            },
            deps.storage,
        ))
    }
}


impl MarketInfo {
fn process_open_positions(
    &mut self,
    state: &State,
    market: &MarketInfo,
    unprocessed_open_positions: Vec<PositionId>,
) -> Result<()> {
    // todo: this needs to be split
    let resp: PositionsResp = state.querier.query_wasm_smart(
        &market.addr,
        &MarketQueryMsg::Positions {
            position_ids: unprocessed_open_positions,
            skip_calc_pending_fees: None,
            fees: Some(PositionsQueryFeeApproach::Accumulated),
            price: None,
        },
    )?;
    let open_positions = resp.positions.into_iter().map(|position| PositionInfo {
        id: position.id,
        active_collateral: position.active_collateral,
        pnl_collateral: position.pnl_collateral,
        pnl_usd: position.pnl_usd,
    });
    // todo: Push resp.closed into pending_closed_positions todo: Push
    // resp.pending_close into pending_close position. The reason
    // being you have do another smart query using the same api which will give pending_closed anyway.
    todo!()
}

}
