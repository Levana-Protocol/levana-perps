use anyhow::Result;
use cosmwasm_std::{entry_point, Event};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, StdResult};
use cw2::{get_contract_version, set_contract_version};
use perpswap::contracts::tracker::entry::{InstantiateMsg, MigrateMsg};
use perpswap::contracts::tracker::events::NewTracker;

use crate::state::ADMINS;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:tracker";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    InstantiateMsg {}: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    ADMINS.save(deps.storage, &info.sender, &())?;

    let event: Event = NewTracker {
        admin: info.sender.into_string(),
    }
    .into();
    Ok(Response::new().add_event(event))
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, MigrateMsg {}: MigrateMsg) -> StdResult<Response> {
    let version = get_contract_version(deps.storage)?;
    if version.contract != CONTRACT_NAME {
        return Err(StdError::generic_err("Can only upgrade from same type"));
    }

    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}
