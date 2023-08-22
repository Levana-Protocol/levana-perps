use crate::state::market::ReplyContext;
use crate::state::pyth::set_pyth_config;

use super::state::{State, StateContext};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, QueryResponse, Reply, Response};
use cw2::{get_contract_version, set_contract_version};
use msg::contracts::pyth_bridge::entry::{
    Config, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use msg::prelude::*;
use semver::Version;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:pyth_bridge";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    InstantiateMsg {
        factory,
        pyth,
        feed_type,
        update_age_tolerance_seconds,
        market,
        feeds,
        feeds_usd,
    }: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    set_pyth_config(
        deps.storage,
        &Config {
            factory: factory.validate(deps.api)?,
            pyth: pyth.validate(deps.api)?,
            feed_type,
            update_age_tolerance_seconds,
            market,
            feeds,
            feeds_usd,
        },
    )?;

    let (_state, ctx) = StateContext::new(deps, env)?;

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    match msg {
        ExecuteMsg::UpdatePrice {
            execs,
            rewards,
            bail_on_error,
        } => {
            // any user may call UpdatePrice, and they get the crank rewards (if any)
            let reward_addr = rewards.unwrap_or_else(|| info.sender.into());
            state.update_market_price(&mut ctx, execs, reward_addr, bail_on_error)?;
        }
    }

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    if msg.id != ReplyContext::ID {
        bail!("invalid reply id");
    }

    state.handle_reply(&mut ctx, msg.result.into_result())?;

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, _store) = State::new(deps, env)?;

    match msg {
        QueryMsg::Version {} => get_contract_version(deps.storage)?.query_result(),
        QueryMsg::Config {} => state.config.query_result(),
        QueryMsg::MarketPrice {
            age_tolerance_seconds,
        } => state
            .market_price(age_tolerance_seconds.into())?
            .query_result(),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response> {
    let old_cw2 = get_contract_version(deps.storage)?;
    let old_version: Version = old_cw2
        .version
        .parse()
        .map_err(|_| anyhow!("couldn't parse old contract version"))?;
    let new_version: Version = CONTRACT_VERSION
        .parse()
        .map_err(|_| anyhow!("couldn't parse new contract version"))?;

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
