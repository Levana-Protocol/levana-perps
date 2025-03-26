use cw_storage_plus::PrefixBound;
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
            let user_addr = deps.api.addr_validate(&user)?;
            let pending = state::USER_WITHDRAWALS
                .prefix_range(
                    deps.storage,
                    Some(PrefixBound::inclusive(&user_addr)),
                    Some(PrefixBound::inclusive(&user_addr)),
                    cosmwasm_std::Order::Ascending,
                )
                .filter_map(|item| {
                    let ((_, queue_id), _) = item.ok()?;
                    let req = state::WITHDRAWAL_QUEUE.load(deps.storage, queue_id).ok()?;
                    Some(req.amount)
                })
                .sum::<Uint128>();
            let response = PendingWithdrawalResponse { amount: pending };
            Ok(to_json_binary(&response)?)
        }

        QueryMsg::GetTotalAssets {} => {
            let response = get_total_assets(deps, &env)?;
            Ok(to_json_binary(&response)?)
        }

        QueryMsg::GetMarketAllocations { start_after } => {
            let response = get_market_allocations(deps, start_after)?;
            Ok(to_json_binary(&response)?)
        }

        QueryMsg::GetConfig {} => Ok(to_json_binary(&state::CONFIG.load(deps.storage)?)?),
    }
}
