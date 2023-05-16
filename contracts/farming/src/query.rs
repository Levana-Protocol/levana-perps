use crate::prelude::*;
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, Env};

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, store) = State::new(deps, env)?;
    match msg {
        QueryMsg::Version {} => todo!(),
        QueryMsg::Status {} => state.get_status(store)?.query_result(),
        QueryMsg::FarmerStats { addr } => {
            let farmer = addr.validate(state.api)?;
            let raw = state.load_raw_farmer_stats(store, &farmer)?;
            FarmerStats {
                farming_tokens: raw.farming_tokens,
                lockdrops: state.get_farmer_lockdrop_stats(store, &farmer)?,
                farming_tokens_available: raw.farming_tokens,
                // FIXME emissions support
                lockdrop_available: "0".parse().unwrap(),
                lockdrop_locked: "0".parse().unwrap(),
                emissions: "0".parse().unwrap(),
            }
            .query_result()
        }
        QueryMsg::Farmers { .. } => todo!(),
    }
}
