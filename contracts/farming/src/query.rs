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
            let farmer_stats = state.load_raw_farmer_stats(store, &farmer)?;
            let emissions = state.may_load_lvn_emissions(store)?;

            let unlocked = match emissions {
                None => LvnToken::zero(),
                Some(emissions) =>state
                    .calculate_unlocked_rewards(store, &farmer_stats, &emissions)?
            };

            let emission_rewards = unlocked.checked_add(farmer_stats.accrued_emissions)?;

            FarmerStats {
                farming_tokens: farmer_stats.total_farming_tokens()?,
                lockdrops: state.get_farmer_lockdrop_stats(store, &farmer)?,
                farming_tokens_available: "0".parse().unwrap(),
                // FIXME emissions support
                lockdrop_available: "0".parse().unwrap(),
                lockdrop_locked: "0".parse().unwrap(),
                emission_rewards,
            }
            .query_result()
        }

        QueryMsg::Farmers { .. } => todo!(),
    }
}
