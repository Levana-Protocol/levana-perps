use perpswap::contracts::vault::QueryMsg;

use crate::{
    common::{get_market_allocations, get_total_assets, get_vault_balance},
    prelude::*,
    state,
    types::PendingWithdrawalResponse,
};
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
            let response = get_vault_balance(deps, &env)?;
            to_json_binary(&response)
        }

        QueryMsg::GetPendingWithdrawal { user } => {
            let pending = state::PENDING_WITHDRAWALS
                .may_load(deps.storage, &user)?
                .unwrap_or(Uint128::zero());
            let response = PendingWithdrawalResponse { amount: pending };
            to_json_binary(&response) // Return user's pending withdrawal
        }

        QueryMsg::GetTotalAssets {} => {
            let response = get_total_assets(deps, &env)?;
            to_json_binary(&response)
        } // Return total assets

        QueryMsg::GetMarketAllocations { start_after, limit } => {
            let response = get_market_allocations(deps, start_after, limit)?;
            to_json_binary(&response)
        }

        QueryMsg::GetConfig {} => to_json_binary(&state::CONFIG.load(deps.storage)?), // Return configuration
    }
}
