use crate::prelude::*;
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, Env};

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, store) = State::new(deps, env)?;

    match msg {
        QueryMsg::Version {} => get_contract_version(store)?.query_result(),
        QueryMsg::Status {} => state.get_status(store)?.query_result(),
        QueryMsg::FarmerStats { addr } => {
            let farmer = addr.validate(state.api)?;

            match state.load_raw_farmer_stats(store, &farmer)? {
                None => FarmerStats::default(),
                Some(farmer_stats) => {
                    let (
                        lockdrop_rewards_available,
                        lockdrop_rewards_locked,
                        lockdrop_deposit_locked,
                    ) = match state.get_period_resp(store)? {
                        FarmingPeriodResp::Launched { .. } => {
                            let lockup_info = state.lockdrop_lockup_info(store, &farmer)?;

                            let lockdrop_rewards =
                                state.calculate_lockdrop_rewards(store, &farmer)?;
                            let lockdrop_rewards_available = state
                                .calculate_unlocked_lockdrop_rewards(
                                    store,
                                    &farmer,
                                    &farmer_stats,
                                )?;
                            let lockdrop_rewards_locked =
                                lockdrop_rewards.checked_sub(lockdrop_rewards_available)?;
                            (
                                lockdrop_rewards_available,
                                lockdrop_rewards_locked,
                                lockup_info.locked,
                            )
                        }
                        _ => (
                            LvnToken::zero(),
                            LvnToken::zero(),
                            farmer_stats.farming_tokens,
                        ),
                    };

                    let farming_tokens_available = farmer_stats
                        .farming_tokens
                        .checked_sub(lockdrop_deposit_locked)?;

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
                        lockdrop_rewards_available,
                        lockdrop_rewards_locked,
                        emission_rewards,
                    }
                }
            }
            .query_result()
        }

        QueryMsg::Farmers { start_after, limit } => {
            let start_after = match start_after {
                None => None,
                Some(addr) => Some(addr.validate(state.api)?),
            };

            state
                .query_farmers(store, start_after, limit)?
                .query_result()
        }
    }
}
