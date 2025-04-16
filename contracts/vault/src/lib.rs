mod common;
mod execute;
mod prelude;
mod query;
mod state;
pub mod types;

pub use execute::execute;
use perpswap::contracts::vault::{Config, InstantiateMsg};
use prelude::*;
pub use query::query;

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response> {
    let governance = deps.api.addr_validate(&msg.governance)?;

    // Ensure the sum of allocation percentages does not exceed 100% (10,000 bps)
    let total_bps: u16 = msg.markets_allocation_bps.values().sum();
    if total_bps > 10_000 {
        return Err(anyhow!("Yield allocation exceeds 100%"));
    }

    let config = Config {
        governance,
        markets_allocation_bps: msg.markets_allocation_bps,
        usdc_denom: msg.usdc_denom,
        paused: false,
    };

    state::CONFIG.save(deps.storage, &config)?;
    state::TOTAL_LP_SUPPLY.save(deps.storage, &Uint128::zero())?;
    state::TOTAL_PENDING_WITHDRAWALS.save(deps.storage, &Uint128::zero())?;

    Ok(Response::new().add_attribute("action", "instantiate_vault"))
}
