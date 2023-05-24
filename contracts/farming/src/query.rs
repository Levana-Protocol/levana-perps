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
            let prefix_sum = state.calculate_rewards_per_token_per_time(store)?;
            let emission_rewards = state
                .calculate_unlocked_rewards(&farmer_stats, prefix_sum)?
                .checked_add(farmer_stats.accrued_emissions)?;

            FarmerStats {
                farming_tokens: farmer_stats.total_farming_tokens()?,
                // FIXME lockdrop support
                farming_tokens_available: farmer_stats.lockdrop_farming_tokens,
                lockdrops: vec![],
                lockdrop_available: "0".parse().unwrap(),
                lockdrop_locked: "0".parse().unwrap(),
                emission_rewards,
            }
            .query_result()
        }

        QueryMsg::Farmers { .. } => todo!(),
    }
}
