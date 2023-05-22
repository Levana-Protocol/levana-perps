use super::state::*;
use crate::state::config::{config_init, load_config, update_config};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    Deps, DepsMut, Env, IbcBasicResponse, IbcChannelCloseMsg, IbcChannelConnectMsg,
    IbcChannelOpenMsg, IbcChannelOpenResponse, IbcPacketAckMsg, IbcPacketReceiveMsg,
    IbcPacketTimeoutMsg, IbcReceiveResponse, MessageInfo, QueryResponse, Response,
};
use cw2::{get_contract_version, set_contract_version};
use msg::contracts::rewards::entry::QueryMsg::{Config as ConfigQuery, RewardsInfo};
use msg::contracts::rewards::entry::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, RewardsInfoResp,
};
use semver::Version;
use shared::prelude::*;

const CONTRACT_NAME: &str = "rewards";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    config_init(deps.api, deps.storage, msg.config)?;

    let (_, ctx) = StateContext::new(deps, env)?;

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    match msg {
        ExecuteMsg::GrantRewards { address, amount } => {
            state.grant_rewards(&mut ctx, address.validate(state.api)?, amount)?;
        }
        ExecuteMsg::ConfigUpdate { config } => {
            let current_config = load_config(ctx.storage)?;

            assert_auth(
                &current_config.factory_addr,
                &state.querier,
                &info.sender,
                AuthCheck::Owner,
            )?;

            update_config(state, ctx.storage, config)?;
        }
        ExecuteMsg::Claim {} => {
            state.claim_rewards(&mut ctx, info.sender)?;
        }
    }

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, store) = State::new(deps, env)?;

    match msg {
        RewardsInfo { addr } => {
            let addr = addr.validate(state.api)?;
            let rewards_info = state.load_rewards(store, &addr)?;

            let res = match rewards_info {
                None => None,
                Some(rewards_info) => match rewards_info.vesting_rewards {
                    None => None,
                    Some(vesting_rewards) => {
                        let unlocked = vesting_rewards.calculate_unlocked_rewards(state.now())?;
                        let locked = vesting_rewards
                            .amount
                            .checked_sub(unlocked)?
                            .checked_sub(vesting_rewards.claimed)?;

                        Some(RewardsInfoResp {
                            locked,
                            unlocked,
                            total_rewards: rewards_info.total_rewards,
                            total_claimed: rewards_info.total_claimed,
                            start: vesting_rewards.start,
                            end: vesting_rewards.start + vesting_rewards.duration,
                        })
                    }
                },
            };

            res.query_result()
        }

        ConfigQuery {} => {
            let config = load_config(store)?;
            config.query_result()
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, _msg: MigrateMsg) -> Result<Response> {
    // Note, we use _state instead of state to avoid warnings when compiling without the sanity
    // feature
    let (_state, ctx) = StateContext::new(deps, env)?;

    let old_cw2 = get_contract_version(ctx.storage)?;
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
        set_contract_version(ctx.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

        Ok(attr_map! {
            "old_contract_name" => old_cw2.contract,
            "old_contract_version" => old_cw2.version,
            "new_contract_name" => CONTRACT_NAME,
            "new_contract_version" => CONTRACT_VERSION,
        })
    }
}

/// Handles the `OpenInit` and `OpenTry` parts of the IBC handshake.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_open(
    deps: DepsMut,
    env: Env,
    msg: IbcChannelOpenMsg,
) -> Result<IbcChannelOpenResponse> {
    let (state, _) = StateContext::new(deps, env)?;
    state.handle_ibc_channel_open(msg)?;
    Ok(None)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_connect(
    deps: DepsMut,
    env: Env,
    msg: IbcChannelConnectMsg,
) -> Result<IbcBasicResponse> {
    let (mut state, mut ctx) = StateContext::new(deps, env)?;
    state.handle_ibc_channel_connect(&mut ctx, msg)?;
    Ok(ctx.response.into_ibc_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_close(
    deps: DepsMut,
    env: Env,
    msg: IbcChannelCloseMsg,
) -> Result<IbcBasicResponse> {
    let (mut state, mut ctx) = StateContext::new(deps, env)?;
    state.handle_ibc_channel_close(&mut ctx, msg)?;
    Ok(ctx.response.into_ibc_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse> {
    let (state, mut ctx) = StateContext::new(deps, env)?;
    let resp = state.handle_ibc_packet_receive(&mut ctx, msg);

    // Regardless of if our processing of this packet works we need to
    // commit an ACK to the chain. As such, we wrap all handling logic
    // in a seprate function and on error write out an error ack.
    // TODO: reconsider https://github.com/CosmWasm/cosmwasm/blob/main/IBC.md#acknowledging-errors
    match resp {
        Ok(_) => Ok(ctx.response.into_ibc_recv_response_success()),
        Err(error) => Ok(ResponseBuilder::new(get_contract_version(ctx.storage)?)
            .into_ibc_recv_response_fail(error)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(deps: DepsMut, env: Env, ack: IbcPacketAckMsg) -> Result<IbcBasicResponse> {
    let (state, mut ctx) = StateContext::new(deps, env)?;
    state.handle_ibc_packet_ack(&mut ctx, ack)?;
    Ok(ctx.response.into_ibc_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketTimeoutMsg,
) -> Result<IbcBasicResponse> {
    let (state, ctx) = StateContext::new(deps, env)?;
    state.handle_ibc_packet_timeout(msg)?;
    Ok(ctx.response.into_ibc_response())
}
