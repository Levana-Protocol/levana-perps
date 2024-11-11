use crate::state::{
    all_contracts::ALL_CONTRACTS,
    auth::{
        get_admin_migration, get_dao, get_kill_switch, get_owner, get_wind_down,
        set_admin_migration, set_dao, set_kill_switch, set_owner, set_wind_down,
    },
    code_ids::get_code_ids,
    label::{get_label_suffix, set_label_suffix},
    liquidity_token::{
        liquidity_token_addr, liquidity_token_code_id, save_liquidity_token_addr,
        set_liquidity_token_code_id,
    },
    market::{
        get_market_addr, get_market_code_id, markets, save_market_addr, set_market_code_id,
        MARKET_ADDRS,
    },
    position_token::{
        position_token_addr, position_token_code_id, save_position_token_addr,
        set_position_token_code_id,
    },
    referrer::{list_referee_count, list_referees_for, set_referrer_for},
    reply::{
        reply_get_instantiate_market, reply_set_instantiate_market, InstantiateMarket, ReplyId,
    },
    shutdown::{get_shutdown_status, shutdown},
};

use super::state::*;
use anyhow::Result;
use copy_trading::{store_new_copy_trading_contract, COPY_TRADING_CODE_ID};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Deps, DepsMut, Env, MessageInfo, QueryResponse, Reply, Response,
};
use cw2::{get_contract_version, set_contract_version};
use perpswap::contracts::{
    factory::{
        entry::{
            AddrIsContractResp, ContractType, CopyTradingAddr, CopyTradingInfo, CopyTradingResp,
            ExecuteMsg, FactoryOwnerResp, GetReferrerResp, InstantiateMsg, LeaderAddr,
            ListRefereeCountStartAfter, MarketInfoResponse, MigrateMsg, QueryMsg, RefereeCount,
            QUERY_LIMIT_DEFAULT,
        },
        events::{InstantiateEvent, NewContractKind},
    },
    liquidity_token::LiquidityTokenKind,
    market::entry::{ExecuteMsg as MarketExecuteMsg, NewCopyTradingParams, NewMarketParams},
};
use perpswap::prelude::*;
use reply::{InstantiateCopyTrading, INSTANTIATE_COPY_TRADING};
use semver::Version;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:factory";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    InstantiateMsg {
        market_code_id,
        position_token_code_id,
        liquidity_token_code_id,
        migration_admin,
        owner,
        dao,
        kill_switch,
        wind_down,
        label_suffix,
        copy_trading_code_id,
    }: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    set_market_code_id(deps.storage, market_code_id.parse()?)?;
    set_position_token_code_id(deps.storage, position_token_code_id.parse()?)?;
    set_liquidity_token_code_id(deps.storage, liquidity_token_code_id.parse()?)?;
    set_owner(deps.storage, &owner.validate(deps.api)?)?;
    set_dao(deps.storage, &dao.validate(deps.api)?)?;
    set_admin_migration(deps.storage, &migration_admin.validate(deps.api)?)?;
    set_kill_switch(deps.storage, &kill_switch.validate(deps.api)?)?;
    set_wind_down(deps.storage, &wind_down.validate(deps.api)?)?;
    set_label_suffix(deps.storage, label_suffix.as_deref().unwrap_or_default())?;
    if let Some(copy_trading_code_id) = copy_trading_code_id {
        let code_id: u64 = copy_trading_code_id.parse()?;
        crate::state::copy_trading::COPY_TRADING_CODE_ID.save(deps.storage, &code_id)?;
    }

    ALL_CONTRACTS.save(deps.storage, &env.contract.address, &ContractType::Factory)?;

    let (_state, ctx) = StateContext::new(deps, env)?;
    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    if msg.requires_owner() && Some(info.sender.clone()) != get_owner(deps.storage)? {
        perp_bail!(
            ErrorId::Auth,
            ErrorDomain::Default,
            "{} is not the auth contract owner",
            info.sender
        )
    }

    execute_msg(deps, env, Some(info), msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: ExecuteMsg) -> Result<Response> {
    if get_owner(deps.storage)?.is_some() {
        perp_bail!(
            ErrorId::Auth,
            ErrorDomain::Default,
            "Sudo entrypoint is only available for the factory which does not have owner",
        )
    }

    execute_msg(deps, env, None, msg)
}

fn execute_msg(
    deps: DepsMut,
    env: Env,
    info: Option<MessageInfo>,
    msg: ExecuteMsg,
) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    match msg {
        ExecuteMsg::AddMarket {
            new_market:
                NewMarketParams {
                    market_id,
                    token,
                    config,
                    initial_borrow_fee_rate,
                    spot_price,
                    initial_price,
                },
        } => {
            if get_market_addr(ctx.storage, &market_id).is_ok() {
                return Err(anyhow!("market already exists for {market_id}"));
            }
            let migration_admin: Addr = get_admin_migration(ctx.storage)?;

            reply_set_instantiate_market(
                ctx.storage,
                InstantiateMarket {
                    market_id: market_id.clone(),
                    migration_admin: migration_admin.clone(),
                },
            )?;

            let label_suffix = get_label_suffix(ctx.storage)?;

            ctx.response.add_instantiate_submessage(
                ReplyId::InstantiateMarket,
                &migration_admin,
                get_market_code_id(ctx.storage)?,
                format!("Levana Perps Market - {market_id}{label_suffix}"),
                &perpswap::contracts::market::entry::InstantiateMsg {
                    factory: state.env.contract.address.into(),
                    config,
                    market_id,
                    token,
                    initial_borrow_fee_rate,
                    spot_price,
                    initial_price,
                },
            )?;
        }

        ExecuteMsg::AddCopyTrading {
            new_copy_trading: NewCopyTradingParams { name, description },
        } => {
            let info = info.context("Message info should be provided")?;
            let leader = info.sender;
            let migration_admin: Addr = get_admin_migration(ctx.storage)?;
            INSTANTIATE_COPY_TRADING.save(
                ctx.storage,
                &InstantiateCopyTrading {
                    migration_admin: migration_admin.clone(),
                    leader: leader.clone(),
                },
            )?;
            let label_suffix = get_label_suffix(ctx.storage)?;
            let copy_trading_code_id = crate::state::copy_trading::COPY_TRADING_CODE_ID
                .may_load(ctx.storage)?
                .context("copy_trading code id is not stored yet")?;
            ctx.response.add_instantiate_submessage(
                ReplyId::InstantiateCopyTrading,
                &migration_admin,
                copy_trading_code_id,
                format!("Levana Perps Copy Trading - {label_suffix}"),
                &perpswap::contracts::copy_trading::InstantiateMsg {
                    leader: leader.into(),
                    config: perpswap::contracts::copy_trading::ConfigUpdate {
                        name: Some(name),
                        description: Some(description),
                        commission_rate: None,
                    },
                    parameters: perpswap::contracts::copy_trading::FactoryConfigUpdate {
                        allowed_rebalance_queries: None,
                        allowed_lp_token_queries: None,
                    },
                },
            )?;
        }

        ExecuteMsg::SetMarketCodeId { code_id } => {
            set_market_code_id(ctx.storage, code_id.parse()?)?;
        }
        ExecuteMsg::SetPositionTokenCodeId { code_id } => {
            set_position_token_code_id(ctx.storage, code_id.parse()?)?;
        }
        ExecuteMsg::SetLiquidityTokenCodeId { code_id } => {
            set_liquidity_token_code_id(ctx.storage, code_id.parse()?)?;
        }

        ExecuteMsg::SetCopyTradingCodeId { code_id } => {
            let code_id: u64 = code_id.parse()?;
            COPY_TRADING_CODE_ID.save(ctx.storage, &code_id)?;
        }

        ExecuteMsg::SetOwner { owner } => {
            set_owner(ctx.storage, &owner.validate(state.api)?)?;
        }

        ExecuteMsg::SetMigrationAdmin { migration_admin } => {
            set_admin_migration(ctx.storage, &migration_admin.validate(state.api)?)?;
        }

        ExecuteMsg::SetDao { dao } => {
            set_dao(ctx.storage, &dao.validate(state.api)?)?;
        }

        ExecuteMsg::SetKillSwitch { kill_switch } => {
            set_kill_switch(ctx.storage, &kill_switch.validate(state.api)?)?;
        }

        ExecuteMsg::SetWindDown { wind_down } => {
            set_wind_down(ctx.storage, &wind_down.validate(state.api)?)?;
        }

        ExecuteMsg::TransferAllDaoFees {} => {
            let addrs = MARKET_ADDRS
                .range(ctx.storage, None, None, cosmwasm_std::Order::Ascending)
                .map(|res| res.map(|(_, addr)| addr).map_err(|err| err.into()))
                .collect::<Result<Vec<Addr>>>()?;

            for addr in addrs {
                ctx.response
                    .add_execute_submessage_oneshot(addr, &MarketExecuteMsg::TransferDaoFees {})?;
            }
        }
        ExecuteMsg::Shutdown {
            markets,
            impacts,
            effect,
        } => {
            let info = info.context("Message info should be provided")?;
            shutdown(&mut ctx, &info, markets, impacts, effect)?
        }
        ExecuteMsg::RegisterReferrer { addr } => {
            let info = info.context("Message info should be provided")?;
            let referrer = addr.validate(state.api)?;
            anyhow::ensure!(
                info.sender != referrer,
                "You cannot set yourself as your own referrer"
            );
            set_referrer_for(ctx.storage, &info.sender, &referrer)?;
            ctx.response.add_event(
                Event::new("register-referrer")
                    .add_attribute("referrer", referrer)
                    .add_attribute("referee", info.sender),
            );
        }
    }

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    match ReplyId::try_from(msg.id) {
        Ok(id) => {
            let result = msg.result.into_result().map_err(|msg| anyhow!("{msg}"))?;
            let addr = extract_instantiated_addr(state.api, &result.events)?;

            match id {
                ReplyId::InstantiateMarket => {
                    let InstantiateMarket {
                        market_id,
                        migration_admin,
                    } = reply_get_instantiate_market(ctx.storage)?;
                    save_market_addr(ctx.storage, &market_id, &addr, &state)?;
                    ctx.response.add_event(InstantiateEvent {
                        kind: NewContractKind::Market,
                        market_id: market_id.clone(),
                        addr,
                    });

                    // now that the market fully exists, including for factory market lookups
                    // instantiate the contracts that depend on market

                    let label_suffix = get_label_suffix(ctx.storage)?;

                    let factory = state.env.contract.address.into_string();
                    ctx.response.add_instantiate_submessage(
                        ReplyId::InstantiatePositionToken,
                        &migration_admin,
                        position_token_code_id(ctx.storage)?,
                        format!("Levana Perps Position Token - {market_id}{label_suffix}"),
                        &perpswap::contracts::position_token::entry::InstantiateMsg {
                            factory: factory.clone().into(),
                            market_id: market_id.clone(),
                        },
                    )?;

                    ctx.response.add_instantiate_submessage(
                        ReplyId::InstantiateLiquidityTokenLp,
                        &migration_admin,
                        liquidity_token_code_id(ctx.storage)?,
                        format!("Levana Perps LP Token - {market_id}{label_suffix}"),
                        &perpswap::contracts::liquidity_token::entry::InstantiateMsg {
                            factory: factory.clone().into(),
                            market_id: market_id.clone(),
                            kind: LiquidityTokenKind::Lp,
                        },
                    )?;

                    ctx.response.add_instantiate_submessage(
                        ReplyId::InstantiateLiquidityTokenXlp,
                        &migration_admin,
                        liquidity_token_code_id(ctx.storage)?,
                        format!("Levana Perps xLP Token - {market_id}{label_suffix}"),
                        &perpswap::contracts::liquidity_token::entry::InstantiateMsg {
                            factory: factory.into(),
                            market_id,
                            kind: LiquidityTokenKind::Xlp,
                        },
                    )?;
                }

                ReplyId::InstantiatePositionToken => {
                    // part of market instantiation flow
                    let market_id = reply_get_instantiate_market(ctx.storage)?.market_id;
                    save_position_token_addr(ctx.storage, market_id.clone(), &addr)?;
                    ctx.response.add_event(InstantiateEvent {
                        kind: NewContractKind::Position,
                        market_id,
                        addr,
                    });
                }
                ReplyId::InstantiateLiquidityTokenLp => {
                    // part of market instantiation flow
                    let market_id = reply_get_instantiate_market(ctx.storage)?.market_id;
                    save_liquidity_token_addr(
                        ctx.storage,
                        market_id.clone(),
                        &addr,
                        LiquidityTokenKind::Lp,
                    )?;
                    ctx.response.add_event(InstantiateEvent {
                        kind: NewContractKind::Lp,
                        market_id,
                        addr,
                    });
                }
                ReplyId::InstantiateLiquidityTokenXlp => {
                    // part of market instantiation flow
                    let market_id = reply_get_instantiate_market(ctx.storage)?.market_id;
                    save_liquidity_token_addr(
                        ctx.storage,
                        market_id.clone(),
                        &addr,
                        LiquidityTokenKind::Xlp,
                    )?;
                    ctx.response.add_event(InstantiateEvent {
                        kind: NewContractKind::Xlp,
                        market_id,
                        addr,
                    });
                }
                ReplyId::InstantiateCopyTrading => {
                    let leader = LeaderAddr(
                        INSTANTIATE_COPY_TRADING
                            .may_load(ctx.storage)?
                            .context("No data in INSTANTIATE_COPY_TRADING")?
                            .leader,
                    );
                    let contract = CopyTradingAddr(addr.clone());
                    store_new_copy_trading_contract(ctx.storage, leader.clone(), contract)?;
                    ALL_CONTRACTS.save(ctx.storage, &addr, &ContractType::CopyTrading)?;
                    copy_trading::COPY_TRADING_LAST_ADDED
                        .save(ctx.storage, &Timestamp::from(state.env.block.time))?;
                    ctx.response.add_event(
                        Event::new("instantiate-copy-trading")
                            .add_attribute("leader", leader.0.to_string())
                            .add_attribute("contract", addr.to_string()),
                    );
                }
            }
        }
        _ => {
            return Err(perp_anyhow!(
                ErrorId::InternalReply,
                ErrorDomain::Factory,
                "not a valid reply id: {}",
                msg.id
            ));
        }
    }

    Ok(ctx.response.into_response())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, store) = State::new(deps, env);
    match msg {
        QueryMsg::Version {} => get_contract_version(store)?.query_result(),

        QueryMsg::Markets { start_after, limit } => {
            markets(store, start_after, limit)?.query_result()
        }

        QueryMsg::MarketInfo { market_id } => {
            let market_addr = get_market_addr(store, &market_id)?;
            MarketInfoResponse {
                market_addr,
                position_token: position_token_addr(store, market_id.clone())?,
                liquidity_token_lp: liquidity_token_addr(
                    store,
                    market_id.clone(),
                    LiquidityTokenKind::Lp,
                )?,
                liquidity_token_xlp: liquidity_token_addr(
                    store,
                    market_id,
                    LiquidityTokenKind::Xlp,
                )?,
            }
            .query_result()
        }

        QueryMsg::AddrIsContract { addr } => {
            let addr = addr.validate(state.api)?;

            match ALL_CONTRACTS.may_load(store, &addr)? {
                Some(contract_type) => AddrIsContractResp {
                    is_contract: true,
                    contract_type: Some(contract_type),
                },
                None => AddrIsContractResp {
                    is_contract: false,
                    contract_type: None,
                },
            }
            .query_result()
        }

        QueryMsg::FactoryOwner {} => FactoryOwnerResp {
            owner: get_owner(store)?,
            admin_migration: get_admin_migration(store)?,
            dao: get_dao(store)?,
            kill_switch: get_kill_switch(store)?,
            wind_down: get_wind_down(store)?,
        }
        .query_result(),

        QueryMsg::ShutdownStatus { market_id } => {
            get_shutdown_status(store, &market_id)?.query_result()
        }

        QueryMsg::CodeIds {} => get_code_ids(store)?.query_result(),

        QueryMsg::GetReferrer { addr } => {
            match state.get_referrer_for(store, &addr.validate(state.api)?)? {
                None => GetReferrerResp::NoReferrer {},
                Some(referrer) => GetReferrerResp::HasReferrer { referrer },
            }
            .query_result()
        }
        QueryMsg::ListReferees {
            addr,
            limit,
            start_after,
        } => {
            let referrer = addr.validate(state.api)?;
            let start_after = start_after
                .map(|addr| RawAddr::from(addr).validate(state.api))
                .transpose()?;
            const MAX_LIMIT: u32 = 30;
            let limit = limit.map_or(MAX_LIMIT, |limit| limit.min(MAX_LIMIT));
            list_referees_for(store, &referrer, limit, start_after.as_ref())?.query_result()
        }
        QueryMsg::ListRefereeCount { limit, start_after } => {
            const MAX_LIMIT: u32 = 30;
            let limit = limit.map_or(MAX_LIMIT, |limit| limit.min(MAX_LIMIT));
            let start_after = start_after
                .map(|ListRefereeCountStartAfter { referrer, count }| {
                    referrer
                        .validate(state.api)
                        .map(|referrer| RefereeCount { referrer, count })
                })
                .transpose()?;
            list_referee_count(store, limit, start_after)?.query_result()
        }
        QueryMsg::CopyTrading { start_after, limit } => {
            let limit = limit.map_or(QUERY_LIMIT_DEFAULT, |limit| limit.min(QUERY_LIMIT_DEFAULT));
            let start_after = match start_after {
                Some(start_after) => {
                    let leader = start_after.leader.validate(state.api)?;
                    let contract = start_after.contract.validate(state.api)?;
                    let (leader, contract) = (LeaderAddr(leader), CopyTradingAddr(contract));
                    let copy_trading_id =
                        copy_trading::COPY_TRADING_ADDRS.may_load(store, (leader, contract))?;
                    match copy_trading_id {
                        Some(copy_trading_id) => Some(copy_trading_id),
                        None => {
                            bail!("Invalid combination of leader and contract passed")
                        }
                    }
                }
                None => None,
            };
            let result = copy_trading::COPY_TRADING_ADDRS_REVERSE
                .range(
                    store,
                    start_after.map(Bound::exclusive),
                    None,
                    cosmwasm_std::Order::Ascending,
                )
                .take(limit.try_into()?)
                .map(|res| res.map(|(_, (leader, contract))| CopyTradingInfo { leader, contract }))
                .collect::<Result<Vec<_>, _>>()?;
            let result = CopyTradingResp { addresses: result };
            let result = to_json_binary(&result)?;
            Ok(result)
        }
        QueryMsg::CopyTradingForLeader {
            leader,
            start_after,
            limit,
        } => {
            let limit = limit.map_or(QUERY_LIMIT_DEFAULT, |limit| limit.min(QUERY_LIMIT_DEFAULT));
            let start_after = match start_after {
                Some(start_after) => {
                    let start_after = start_after.validate(state.api)?;
                    Some(CopyTradingAddr(start_after))
                }
                None => None,
            };
            let leader = LeaderAddr(leader.validate(state.api)?);
            let addresses = copy_trading::COPY_TRADING_ADDRS
                .prefix(leader.clone())
                .keys(
                    store,
                    start_after.map(Bound::exclusive),
                    None,
                    cosmwasm_std::Order::Ascending,
                )
                .map(|item| {
                    item.map(|contract| CopyTradingInfo {
                        leader: leader.clone(),
                        contract,
                    })
                })
                .take(limit.try_into()?)
                .collect::<Result<Vec<_>, _>>()?;
            let result = CopyTradingResp { addresses };
            let result = to_json_binary(&result)?;
            Ok(result)
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
