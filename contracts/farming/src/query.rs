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

            let lockup_info = state.lockdrop_lockup_info(store, &farmer)?;
            let farming_tokens_available = farmer_stats
                .farming_tokens
                .checked_sub(lockup_info.locked)?;

            let lockdrop_rewards = state.calculate_lockdrop_rewards(store, &farmer)?;
            let lockdrop_available = state.calculate_unlocked_lockdrop_rewards(store, &farmer)?;
            let lockdrop_locked = lockdrop_rewards.checked_sub(lockdrop_available)?;

            let emissions = state.may_load_lvn_emissions(store)?;
            let unlocked_emissions = match emissions {
                None => LvnToken::zero(),
                Some(emissions) => {
                    state.calculate_unlocked_emissions(store, &farmer_stats, &emissions)?
                }
            };
            let emission_rewards =
                unlocked_emissions.checked_add(farmer_stats.accrued_emissions)?;

            FarmerStats {
                farming_tokens: farmer_stats.farming_tokens,
                lockdrops: state.get_farmer_lockdrop_stats(store, &farmer)?,
                farming_tokens_available,
                lockdrop_available,
                lockdrop_locked,
                emission_rewards,
            }
            .query_result()
        }

        QueryMsg::Farmers { .. } => todo!(),
    }
}
