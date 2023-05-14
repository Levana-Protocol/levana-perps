use crate::prelude::*;
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, Env};

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, store) = State::new(deps, env)?;
    match msg {
        QueryMsg::Version {} => todo!(),
        QueryMsg::Status {} => todo!(),
        QueryMsg::FarmerStats { addr } => {
            let farmer = addr.validate(state.api)?;
            let raw = state.load_raw_farmer_stats(store, &farmer)?;
            FarmerStats {
                farming_tokens: raw.xlp_farming_tokens,
                // FIXME lockdrop support
                farming_tokens_available: raw.xlp_farming_tokens,
                lockdrops: vec![],
                lockdrop_available: "0".parse().unwrap(),
                lockdrop_locked: "0".parse().unwrap(),
                emissions: "0".parse().unwrap(),
            }
            .query_result()
        }
        QueryMsg::Farmers { .. } => todo!(),
    }
}
