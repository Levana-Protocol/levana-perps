use perpswap::contracts::vault::QueryMsg;

use crate::{common::get_total_assets, prelude::*, state};
#[allow(dead_code)]
/// Handles all queries to the vault
///
/// # Parameters
/// - `deps`: Dependencies for storage access
/// - `env`: Contract environment
/// - `msg`: Query message
///
/// # Returns
/// - `StdResult<Binary>`: Serialized query response
#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetVaultBalance {} => {
            let config = state::CONFIG.load(deps.storage)?; // Load configuration
            let balance = deps
                .querier
                .query_balance(&env.contract.address, &config.usdc_denom)?
                .amount;
            to_json_binary(&balance) // Return vault balance
        }
        QueryMsg::GetPendingWithdrawal { user } => {
            let pending = state::PENDING_WITHDRAWALS
                .may_load(deps.storage, &user)?
                .unwrap_or(Uint128::zero());
            to_json_binary(&pending) // Return user's pending withdrawal
        }
        QueryMsg::GetTotalAssets {} => to_json_binary(&get_total_assets(deps, &env)?), // Return total assets
        QueryMsg::GetMarketAllocations { start_after, limit } => {
            let start: Option<Bound<&str>> = start_after.as_deref().map(Bound::exclusive);
            let allocations = state::MARKET_ALLOCATIONS
                .range(deps.storage, start, None, Order::Ascending)
                .take(limit.unwrap_or(30) as usize)
                .map(|item| {
                    let (k, v) = item?;
                    Ok((k.to_string(), v))
                })
                .collect::<StdResult<Vec<_>>>()?;
            to_json_binary(&allocations) // Return list of market allocations
        }
        QueryMsg::GetConfig {} => to_json_binary(&state::CONFIG.load(deps.storage)?), // Return configuration
        QueryMsg::IsPaused {} => to_json_binary(&state::PAUSED.load(deps.storage)?), // Return paused state
        QueryMsg::GetOperators {} => to_json_binary(&state::CONFIG.load(deps.storage)?.operators), // Return list of operators
    }
}
