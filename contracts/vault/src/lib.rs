mod common;
mod execute;
mod prelude;
mod query;
mod state;
mod types;

use perpswap::contracts::vault::{Config, InstantiateMsg};
use prelude::*;

/// Instantiates the vault with the provided initial configuration
///
/// # Parameters
/// - `deps`: Mutable dependencies for storage and API access
/// - `_env`: Contract environment (unused here)
/// - `_info`: Message information (unused here)
/// - `msg`: Instantiation message with initial parameters
///
/// # Returns
/// - `StdResult<Response>`: Success response or error if validation or saving fails
#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    // Validate and convert the governance address
    let governance = deps.api.addr_validate(&msg.governance)?;
    // Validate and convert the initial operators' addresses
    let operators = msg
        .initial_operators
        .iter()
        .map(|a| deps.api.addr_validate(a))
        .collect::<StdResult<Vec<_>>>()?;

    // Ensure the sum of allocation percentages does not exceed 100% (10,000 bps)
    let total_bps: u16 = msg.markets_allocation_bps.iter().sum();
    if total_bps > 10_000 {
        return Err(StdError::generic_err("Yield allocation exceeds 100%"));
    }

    // Construct the config with all required fields
    let config = Config {
        governance,
        operators,
        markets_allocation_bps: msg.markets_allocation_bps,
        usdc_denom: msg.usdc_denom,
        usdclp_address: msg.usdclp_address,
        paused: false,
    };

    // Save configuration, initial LP supply, and paused state
    state::CONFIG.save(deps.storage, &config)?;
    state::TOTAL_LP_SUPPLY.save(deps.storage, &Uint128::zero())?; // Starts at 0

    // Return a response with an attribute indicating the action
    Ok(Response::new().add_attribute("action", "instantiate_vault"))
}
