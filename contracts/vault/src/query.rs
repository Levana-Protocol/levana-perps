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
            let mut pending = Uint128::zero();

            for item in state::USER_WITHDRAWALS.prefix(&user_addr).range(
                deps.storage,
                None,
                None,
                cosmwasm_std::Order::Ascending,
            ) {
                let (queue_id, _) = item?;
                let req = state::WITHDRAWAL_QUEUE.load(deps.storage, queue_id)?;
                pending = pending.checked_add(req.amount)?;
            }

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
