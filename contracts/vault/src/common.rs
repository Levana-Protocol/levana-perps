use crate::{
    prelude::*,
    state,
    types::{
        MarketAllocation, MarketAllocationsResponse, TotalAssetsResponse, VaultBalanceResponse,
    },
};
#[allow(dead_code)]
/// Calculates the total initially allocated assets of the vault (balance + allocations).
///
/// This sums the vault's native USDC balance with the initial USDC amounts allocated
/// to markets. Note that the actual value of market allocations may differ due to
/// LP token price changes, impermanent loss, or yield.
///
/// # Parameters
/// - `deps`: Dependencies for storage access
/// - `env`: Contract environment
///
/// # Returns
/// - `StdResult<Uint128>`: Sum of USDC balance and initial market allocations
pub(crate) fn get_total_assets(deps: Deps, env: &Env) -> StdResult<TotalAssetsResponse> {
    let config = state::CONFIG.load(deps.storage)?;
    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;
    let allocated_amount: Uint128 = state::MARKET_ALLOCATIONS
        .range(deps.storage, None, None, Order::Ascending)
        .try_fold(Uint128::zero(), |acc, item| -> Result<Uint128, StdError> {
            Ok(acc + item?.1)
        })?;
    let total_assets = vault_balance
        .checked_add(allocated_amount)
        .map_err(|_| StdError::generic_err("Overflow in total assets calculation"))?;
    Ok(TotalAssetsResponse { total_assets })
}

/// Retrieves the vault's balance details.
///
/// Calculates the native balance, initial allocated amount, pending withdrawals, and total
/// allocated assets of the vault. Note: `allocated_amount` reflects initial USDC allocations,
/// not the current value of LP tokens, which may differ due to trader PnL or impermanent loss.
///
/// # Arguments
/// * `deps` - Immutable dependencies for storage and querying.
/// * `env` - Environment info providing contract address and block details.
///
/// # Returns
/// The vault balance details wrapped in a response object.
pub(crate) fn get_vault_balance(deps: Deps, env: &Env) -> StdResult<VaultBalanceResponse> {
    let config = state::CONFIG.load(deps.storage)?;
    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;
    let allocated_amount = state::MARKET_ALLOCATIONS
        .range(deps.storage, None, None, Order::Ascending)
        .try_fold(Uint128::zero(), |acc, res| -> Result<Uint128, StdError> {
            Ok(acc + res?.1)
        })?;
    let pending_withdrawals = state::PENDING_WITHDRAWALS
        .range(deps.storage, None, None, Order::Ascending)
        .try_fold(Uint128::zero(), |acc, res| -> Result<Uint128, StdError> {
            Ok(acc + res?.1)
        })?;
    let total_allocated = vault_balance
        .checked_add(allocated_amount)
        .map_err(|_| StdError::generic_err("Overflow in total calculation"))?;

    Ok(VaultBalanceResponse {
        vault_balance,
        allocated_amount,
        pending_withdrawals,
        total_allocated,
    })
}

/// Retrieves a paginated list of initial market allocations.
///
/// # Arguments
/// * `deps` - Immutable dependencies for storage and querying.
/// * `start_after` - Optional starting point for pagination (exclusive).
/// * `limit` - Optional maximum number of entries to return (default 30).
///
/// # Returns
/// A list of initial market allocations. Note: Amounts reflect initial USDC allocations,
/// not current LP values, which may vary due to trader PnL or impermanent loss.
pub(crate) fn get_market_allocations(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MarketAllocationsResponse> {
    let start: Option<Bound<&str>> = start_after.as_deref().map(Bound::exclusive);
    let allocations = state::MARKET_ALLOCATIONS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit.unwrap_or(30) as usize)
        .map(|item| {
            let (market_id, amount) = item?;
            Ok(MarketAllocation {
                market_id: market_id.to_string(),
                amount,
            })
        })
        .collect::<StdResult<Vec<_>>>()?;
    Ok(MarketAllocationsResponse { allocations })
}

/// Ensures the contract is not paused
///
/// # Parameters
/// - `deps`: Dependencies for storage access
///
/// # Returns
/// - `StdResult<()>`: `Ok(())` if not paused, error if paused
pub(crate) fn check_not_paused(deps: &Deps) -> StdResult<()> {
    // Load the current configuration from storage
    let config = state::CONFIG.load(deps.storage)?;

    if config.paused {
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
