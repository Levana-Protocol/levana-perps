use super::state::{pyth::set_pyth_addr, set_factory_addr, State, StateContext};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, QueryResponse, Response};
use cw2::{get_contract_version, set_contract_version};
use msg::contracts::pyth_bridge::entry::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
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
    msg: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    set_factory_addr(deps.storage, &msg.factory.validate(deps.api)?)?;
    set_pyth_addr(deps.storage, &msg.pyth.validate(deps.api)?)?;

    let (state, mut ctx) = StateContext::new(deps, env)?;

    state.set_pyth_update_age_tolerance(&mut ctx, msg.update_age_tolerance_seconds.into())?;

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    if msg.requires_admin() {
        assert_auth(
            &state.factory_address,
            &state.querier,
            &info.sender,
            AuthCheck::Owner,
        )?;
    }

    match msg {
        ExecuteMsg::SetMarketPriceFeeds {
            market_id,
            market_price_feeds,
        } => {
            state.set_pyth_market_price_feeds(&mut ctx, market_id, market_price_feeds)?;
        }

        ExecuteMsg::SetUpdateAgeTolerance { seconds } => {
            state.set_pyth_update_age_tolerance(&mut ctx, seconds.into())?;
        }

        ExecuteMsg::UpdatePrice {
            market_id,
            execs,
            rewards,
            bail_on_error,
        } => {
            // any user may call UpdatePrice, and they get the crank rewards (if any)
            let reward_addr = rewards.unwrap_or_else(|| info.sender.into());
            if let Err(err) = state.update_market_price(&mut ctx, market_id, execs, reward_addr) {
                if bail_on_error {
                    return Err(err);
                } else {
                    ctx.response.set_data(&err.to_string())?;
                }
            }
        }
    }

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, store) = State::new(deps, env)?;

    match msg {
        QueryMsg::Version {} => get_contract_version(deps.storage)?.query_result(),
        QueryMsg::PythAddress {} => state.get_pyth_addr(store)?.query_result(),
        QueryMsg::MarketPriceFeeds { market_id } => state
            .get_pyth_market_price_feeds(store, &market_id)?
            .query_result(),
        QueryMsg::AllMarketPriceFeeds {
            start_after,
            limit,
            order,
        } => state
            .get_all_pyth_market_price_feeds(
                store,
                start_after.as_ref(),
                limit,
                order.map(|x| x.into()),
            )?
            .query_result(),
        QueryMsg::MarketPrice {
            market_id,
            age_tolerance_seconds,
        } => state
            .market_price(store, &market_id, age_tolerance_seconds.into())?
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
