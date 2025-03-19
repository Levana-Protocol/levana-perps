use crate::{prelude::*, state};
#[allow(dead_code)]
/// Calculates the total assets of the vault (balance + allocations)
///
/// # Parameters
/// - `deps`: Dependencies for storage access
/// - `env`: Contract environment
///
/// # Returns
/// - `StdResult<Uint128>`: Sum of USDC balance and market allocations
pub(super) fn get_total_assets(deps: Deps, env: &Env) -> StdResult<Uint128> {
    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;
    let allocated: Uint128 = state::MARKET_ALLOCATIONS
        .range(deps.storage, None, None, Order::Ascending)
        .try_fold(Uint128::zero(), |acc, item| -> Result<Uint128, StdError> {
            Ok(acc + item?.1)
        })?;

    Ok(vault_balance + allocated) // Sum balance and allocations
}

/// Ensures the contract is not paused
///
/// # Parameters
/// - `deps`: Dependencies for storage access
///
/// # Returns
/// - `StdResult<()>`: `Ok(())` if not paused, error if paused
pub(crate) fn check_not_paused(deps: &Deps) -> StdResult<()> {
    if state::PAUSED.load(deps.storage)? {
        return Err(StdError::generic_err("Contract operations are paused"));
    }
    Ok(())
}

/// Checks if the sender is authorized (governance or operator)
///
/// # Parameters
/// - `deps`: Dependencies for storage access
/// - `sender`: Address of the sender to check
///
/// # Returns
/// - `StdResult<bool>`: `true` if authorized, `false` otherwise
pub(crate) fn is_authorized(deps: &Deps, sender: &Addr) -> StdResult<bool> {
    let config = state::CONFIG.load(deps.storage)?;
    Ok(sender == config.governance || config.operators.contains(sender))
}
