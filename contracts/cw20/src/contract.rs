use super::state::*;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, QueryResponse, Response};
use cw2::{get_contract_version, set_contract_version};
use perpswap::contracts::cw20::entry::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use perpswap::{attr_map, prelude::*};
use semver::Version;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:cw20";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let (state, mut ctx) = StateContext::new(deps, env)?;

    state.token_init(&mut ctx, msg)?;

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    state.assert_trading_competition(&mut ctx, &info.sender, &msg)?;

    match msg {
        ExecuteMsg::SetMarket { addr } => {
            let minter_addr = state.minter_addr(ctx.storage)?;

            if minter_addr != info.sender {
                return Err(anyhow!(
                    "Cannot Setmarket, sender is {}, minter is {minter_addr}",
                    info.sender
                ));
            }

            state.set_market_addr(&mut ctx, &addr.validate(state.api)?)?;
        }

        ExecuteMsg::Transfer { recipient, amount } => {
            state.transfer(
                &mut ctx,
                info.sender,
                recipient.validate(state.api)?,
                amount,
            )?;
        }

        ExecuteMsg::Burn { amount } => {
            state.burn(&mut ctx, info.sender, amount)?;
        }

        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => {
            state.send_with_msg(
                &mut ctx,
                info.sender,
                contract.validate(state.api)?,
                amount,
                msg,
            )?;
        }

        ExecuteMsg::Mint { recipient, amount } => {
            state.mint(
                &mut ctx,
                info.sender,
                recipient.validate(state.api)?,
                amount,
            )?;
        }

        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => {
            state.increase_allowance(
                &mut ctx,
                info.sender,
                spender.validate(state.api)?,
                amount,
                expires,
            )?;
        }

        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => {
            state.decrease_allowance(
                &mut ctx,
                info.sender,
                spender.validate(state.api)?,
                amount,
                expires,
            )?;
        }

        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => {
            state.transfer_from(
                &mut ctx,
                info.sender,
                owner.validate(state.api)?,
                recipient.validate(state.api)?,
                amount,
            )?;
        }

        ExecuteMsg::BurnFrom { owner, amount } => {
            state.burn_from(&mut ctx, info.sender, owner.validate(state.api)?, amount)?;
        }

        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => {
            state.send_with_msg_from(
                &mut ctx,
                info.sender,
                owner.validate(state.api)?,
                contract.validate(state.api)?,
                amount,
                msg,
            )?;
        }

        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => {
            state.set_marketing(&mut ctx, info.sender, project, description, marketing)?;
        }

        ExecuteMsg::UploadLogo(logo) => {
            state.set_logo(&mut ctx, info.sender, logo)?;
        }

        ExecuteMsg::UpdateMinter { new_minter } => {
            state.set_minter(&mut ctx, info.sender, new_minter)?;
        }
    }

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, store) = State::new(deps, env)?;

    match msg {
        QueryMsg::Balance { address } => state
            .balance(store, &address.validate(state.api)?)?
            .query_result(),

        QueryMsg::TokenInfo {} => state.token_info(store)?.query_result(),

        QueryMsg::Minter {} => state.minter_resp(store)?.query_result(),

        QueryMsg::Allowance { owner, spender } => state
            .allowance(
                store,
                &owner.validate(state.api)?,
                &spender.validate(state.api)?,
            )?
            .query_result(),

        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => state
            .owner_allowances(
                store,
                owner.validate(state.api)?,
                start_after.map(|x| x.validate(state.api)).transpose()?,
                limit,
            )?
            .query_result(),

        QueryMsg::AllSpenderAllowances {
            spender,
            start_after,
            limit,
        } => state
            .spender_allowances(
                store,
                spender.validate(state.api)?,
                start_after.map(|x| x.validate(state.api)).transpose()?,
                limit,
            )?
            .query_result(),

        QueryMsg::AllAccounts { start_after, limit } => state
            .all_accounts(
                store,
                start_after.map(|x| x.validate(state.api)).transpose()?,
                limit,
            )?
            .query_result(),

        QueryMsg::MarketingInfo {} => state.marketing_info(store)?.query_result(),

        QueryMsg::DownloadLogo {} => state.logo(store)?.query_result(),

        QueryMsg::Version {} => get_contract_version(store)?.query_result(),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response> {
    // let (state, mut ctx) = StateContext::new(deps, env)?;

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
