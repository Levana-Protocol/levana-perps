use perpswap::contracts::vault::QueryMsg;

use crate::{
    common::{get_market_allocations, get_total_assets, get_vault_balance},
    prelude::*,
    state,
    types::PendingWithdrawalResponse,
};

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary> {
    match msg {
        QueryMsg::GetVaultBalance {} => {
            let response = get_vault_balance(deps, &env)?;
            Ok(to_json_binary(&response)?)
        }

        QueryMsg::GetPendingWithdrawal { user } => {
            let pending = state::WITHDRAWAL_QUEUE
                .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
                .filter_map(|item| {
                    let (_, req) = item.ok()?;
                    if req.user.to_string() == user {
                        Some(req.amount)
                    } else {
                        None
                    }
                })
                .sum::<Uint128>();

            let response = PendingWithdrawalResponse { amount: pending };
            Ok(to_json_binary(&response)?)
        }

        QueryMsg::GetTotalAssets {} => {
            let response = get_total_assets(deps, &env)?;
            Ok(to_json_binary(&response)?)
        }

        QueryMsg::GetMarketAllocations { start_after, limit } => {
            let response = get_market_allocations(deps, start_after, limit)?;
            Ok(to_json_binary(&response)?)
        }

        QueryMsg::GetConfig {} => Ok(to_json_binary(&state::CONFIG.load(deps.storage)?)?),
    }
}
