use crate::state::{
    gas_coin::{clear_gas_allowance, get_gas_allowance, set_gas_allowance},
    owner::{add_admin, get_all_admins, is_admin, remove_admin},
    tokens::{
        get_cw20_code_id, get_next_index, get_token, set_cw20_code_id, set_next_token, TokenInfo,
    },
};

use super::state::*;
use anyhow::{anyhow, Context, Result};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    Coin, Deps, DepsMut, Env, MessageInfo, QueryResponse, Reply, Response, Storage,
};
use cw2::{get_contract_version, set_contract_version};
use msg::contracts::{
    cw20::{entry::InstantiateMinter, Cw20Coin},
    faucet::{
        entry::{
            ConfigResponse, ExecuteMsg, FaucetAsset, FundsSentResponse, GasAllowance,
            GasAllowanceResp, GetTokenResponse, IneligibleReason, InstantiateMsg, IsAdminResponse,
            MigrateMsg, NextTradingIndexResponse, OwnerMsg, QueryMsg, TapAmountResponse,
            TapEligibleResponse,
        },
        error::FaucetError,
    },
};
use semver::Version;
use shared::prelude::*;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:faucet";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    InstantiateMsg {
        tap_limit,
        cw20_code_id,
        gas_allowance,
    }: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let (state, mut ctx) = StateContext::new(deps, env)?;

    add_admin(ctx.storage, &info.sender)?;
    state.set_tap_limit(&mut ctx, tap_limit)?;
    set_cw20_code_id(ctx.storage, cw20_code_id)?;
    if let Some(gas_allowance) = gas_allowance {
        set_gas_allowance(ctx.storage, &gas_allowance)?;
    }

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    fn validate_owner(store: &dyn Storage, info: &MessageInfo) -> Result<()> {
        if !is_admin(store, &info.sender) {
            perp_bail!(
                ErrorId::Auth,
                ErrorDomain::Faucet,
                "{} is not owner",
                info.sender
            );
        }

        Ok(())
    }
    match msg {
        ExecuteMsg::Tap {
            assets,
            recipient,
            amount,
        } => {
            let recipient = recipient.validate(state.api)?;

            if amount.is_some() {
                validate_owner(ctx.storage, &info)?;
            }

            let assets = match get_gas_allowance(ctx.storage)? {
                None => assets,
                Some(GasAllowance { denom, amount }) => {
                    let Coin {
                        denom: curr_denom,
                        amount: curr_amount,
                    } = state.querier.query_balance(&recipient, &denom)?;
                    debug_assert_eq!(denom, curr_denom);
                    if curr_amount < amount {
                        state.tap(
                            &mut ctx,
                            FaucetAsset::Native(curr_denom),
                            &recipient,
                            Some(Decimal256::from_atomics(amount - curr_amount, 6)?.into_signed()),
                        )?;
                    }

                    assets
                        .into_iter()
                        .filter(|asset| match asset {
                            FaucetAsset::Cw20(_) => true,
                            FaucetAsset::Native(requested_denom) => &denom != requested_denom,
                        })
                        .collect()
                }
            };

            if !is_admin(ctx.storage, &info.sender) {
                for asset in &assets {
                    state.assert_trading_competition(&mut ctx, recipient.clone(), asset)?;
                }
            }

            state.validate_tap(ctx.storage, &recipient, &assets)?;

            for asset in assets {
                state.tap(&mut ctx, asset, &recipient, amount)?;
            }

            state.save_last_tap(&mut ctx, &recipient)?;
        }
        ExecuteMsg::Multitap { recipients } => {
            state.multitap(&mut ctx, recipients)?;
        }
        ExecuteMsg::OwnerMsg(owner_msg) => {
            validate_owner(ctx.storage, &info)?;

            match owner_msg {
                OwnerMsg::SetTapLimit { tap_limit } => {
                    state.set_tap_limit(&mut ctx, tap_limit)?;
                }
                OwnerMsg::SetTapAmount { asset, amount } => {
                    state.set_tap_amount(&mut ctx, asset, amount)?;
                }
                OwnerMsg::AddAdmin { admin } => {
                    add_admin(ctx.storage, &admin.validate(state.api)?)?;
                }
                OwnerMsg::RemoveAdmin { admin } => {
                    remove_admin(ctx.storage, &admin.validate(state.api)?)?;
                }
                OwnerMsg::DeployToken {
                    name,
                    tap_amount,
                    trading_competition_index,
                    initial_balances,
                } => {
                    set_next_token(
                        ctx.storage,
                        &TokenInfo {
                            name: name.clone(),
                            trading_competition_index,
                            tap_amount,
                        },
                    )?;
                    ctx.response.add_instantiate_submessage(
                        ReplyId,
                        &state.env.contract.address,
                        get_cw20_code_id(ctx.storage)?,
                        match trading_competition_index {
                            Some(i) => format!("Levana Faucet CW20 {name} #{i}"),
                            None => format!("Levana Faucet CW20 {name}"),
                        },
                        &msg::contracts::cw20::entry::InstantiateMsg {
                            name: name.clone(),
                            symbol: name,
                            decimals: 6,
                            initial_balances,
                            minter: InstantiateMinter {
                                minter: state.env.contract.address.clone().into(),
                                cap: None,
                            },
                            marketing: None,
                        },
                    )?;
                }
                OwnerMsg::SetMarketAddress {
                    name,
                    trading_competition_index,
                    market,
                } => {
                    let cw20 = get_token(ctx.storage, &name, Some(trading_competition_index))?
                        .context("CW20 not found")?;
                    ctx.response.add_execute_submessage_oneshot(
                        cw20,
                        &msg::contracts::cw20::entry::ExecuteMsg::SetMarket { addr: market },
                    )?;
                }
                OwnerMsg::SetCw20CodeId { cw20_code_id } => {
                    set_cw20_code_id(ctx.storage, cw20_code_id)?;
                }
                OwnerMsg::Mint { cw20, balances } => {
                    let cw20 = state.api.addr_validate(&cw20)?;
                    for Cw20Coin { address, amount } in balances {
                        let address = state.api.addr_validate(&address)?;
                        ctx.response.add_execute_submessage_oneshot(
                            &cw20,
                            &msg::contracts::cw20::entry::ExecuteMsg::Mint {
                                recipient: address.into(),
                                amount,
                            },
                        )?;
                    }
                }
                OwnerMsg::SetGasAllowance { allowance } => {
                    set_gas_allowance(ctx.storage, &allowance)?
                }
                OwnerMsg::ClearGasAllowance {} => clear_gas_allowance(ctx.storage),
                OwnerMsg::SetMultitapAmount { name, amount } => {
                    state.set_multitap_amount(&mut ctx, &name, amount)?
                }
            }
        }
    }

    Ok(ctx.response.into_response())
}

struct ReplyId;
impl From<ReplyId> for u64 {
    fn from(ReplyId: ReplyId) -> Self {
        0
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    let result = msg.result.into_result().map_err(|msg| anyhow!("{msg}"))?;
    let addr = extract_instantiated_addr(state.api, &result.events)?;
    state.save_next_token(&mut ctx, &addr)?;

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    match msg {
        QueryMsg::Version {} => get_contract_version(deps.storage)?.query_result(),
        QueryMsg::Config {} => {
            let (state, store) = State::new(deps, env);
            ConfigResponse {
                admins: get_all_admins(store)?,
                tap_limit: state.tap_limit(store)?,
            }
            .query_result()
        }
        QueryMsg::GetToken {
            name,
            trading_competition_index,
        } => match get_token(deps.storage, &name, trading_competition_index)? {
            None => GetTokenResponse::NotFound {},
            Some(address) => GetTokenResponse::Found { address },
        }
        .query_result(),
        QueryMsg::NextTradingIndex { name } => NextTradingIndexResponse {
            next_index: get_next_index(deps.storage, &name)?,
        }
        .query_result(),
        QueryMsg::GetGasAllowance {} => get_gas_allowance(deps.storage)?
            .map_or(GasAllowanceResp::Disabled {}, |x| {
                GasAllowanceResp::Enabled {
                    denom: x.denom,
                    amount: x.amount,
                }
            })
            .query_result(),
        QueryMsg::IsTapEligible { addr, assets } => {
            let addr = addr.validate(deps.api)?;
            let (state, store) = State::new(deps, env);
            match state.validate_tap_faucet_error(store, &addr, &assets)? {
                Ok(()) => TapEligibleResponse::Eligible {},
                Err(FaucetError::TooSoon { wait_secs }) => TapEligibleResponse::Ineligible {
                    seconds: wait_secs,
                    message: format!(
                        "You can only tap the faucet again in {}",
                        PrettyTimeRemaining(wait_secs)
                    ),
                    reason: IneligibleReason::TooSoon,
                },
                Err(FaucetError::AlreadyTapped { cw20: _ }) => TapEligibleResponse::Ineligible {
                    seconds: Decimal256::zero(),
                    message: "During the trading competition there is a limit of one faucet tap per person"
                        .to_owned(),
                    reason: IneligibleReason::AlreadyTapped,
                },
            }
            .query_result()
        }
        QueryMsg::IsAdmin { addr } => {
            let addr = addr.validate(deps.api)?;
            IsAdminResponse {
                is_admin: is_admin(deps.storage, &addr),
            }
            .query_result()
        }
        QueryMsg::TapAmount { asset } => {
            let (state, store) = State::new(deps, env);
            match state.get_multitap_amount(store, &asset)? {
                Some(amount) => TapAmountResponse::CanTap { amount },
                None => TapAmountResponse::CannotTap {},
            }
            .query_result()
        }
        QueryMsg::TapAmountByName { name } => {
            let (state, store) = State::new(deps, env);
            match state.get_multitap_amount_by_name(store, &name)? {
                Some(amount) => TapAmountResponse::CanTap { amount },
                None => TapAmountResponse::CannotTap {},
            }
            .query_result()
        }
        QueryMsg::FundsSent { asset, timestamp } => {
            let (state, store) = State::new(deps, env);
            let amount =
                state.get_history(store, &asset, timestamp.unwrap_or_else(|| state.now()))?;
            FundsSentResponse { amount }.query_result()
        }
        QueryMsg::Tappers { start_after, limit } => {
            let (state, store) = State::new(deps, env);
            let start_after = match start_after {
                Some(raw) => Some(raw.validate(deps.api)?),
                None => None,
            };
            state
                .tappers(store, start_after, limit.unwrap_or(30))?
                .query_result()
        }
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
