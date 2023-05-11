use super::state::*;
use anyhow::{anyhow, Result};
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, QueryResponse, Response};
use cw2::{get_contract_version, set_contract_version};
use msg::contracts::position_token::entry::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use msg::prelude::*;
use semver::Version;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:position_token";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, WrappedPerpError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    set_factory_addr(deps.storage, &msg.factory.validate(deps.api)?)?;

    let (state, mut ctx) = StateContext::new(deps, env)?;
    state.market_init(&mut ctx, msg.market_id)?;

    Ok(ctx.response.into_response())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, WrappedPerpError> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    state.market_execute_nft(&mut ctx, info.sender, msg)?;

    Ok(ctx.response.into_response())
}

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse, WrappedPerpError> {
    match msg {
        QueryMsg::Version {} => get_contract_version(deps.storage)?.query_result(),
        _ => {
            let (state, store) = State::new(deps, env)?;
            state.market_query_nft(store, msg)
        }
    }
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, WrappedPerpError> {
    let old_cw2 = get_contract_version(deps.storage)?;
    let old_version: Version = old_cw2
        .version
        .parse()
        .map_err(|_| anyhow!("Couldn't parse old contract version"))?;
    let new_version: Version = CONTRACT_VERSION
        .parse()
        .map_err(|_| anyhow!("Couldn't parse new contract version"))?;

    if old_cw2.contract != CONTRACT_NAME {
        Err(anyhow!(
            "mismatched contract migration name (from {} to {})",
            old_cw2.contract,
            CONTRACT_NAME
        ))
    } else if old_version > new_version {
        Err(anyhow!(
            "cannot migrate contract from newer to older (from {} to {})",
            old_cw2.version,
            CONTRACT_VERSION
        ))
    } else {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        Ok(attr_map! {
            "old_contract_name" => old_cw2.contract,
            "old_contract_version" => old_cw2.version,
            "new_contract_name" => CONTRACT_NAME,
            "new_contract_version" => CONTRACT_VERSION,
        })
    }
}
