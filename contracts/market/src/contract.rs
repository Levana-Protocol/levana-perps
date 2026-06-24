mod perps_info;

use crate::state::{
    config::{config_init, update_config},
    crank::crank_init,
    delta_neutrality_fee::DELTA_NEUTRALITY_FUND,
    fees::fees_init,
    liquidity::{liquidity_init, yield_init},
    meta::meta_init,
    order::backwards_compat_limit_order_take_profit,
    position::{get_position, positions_init},
    set_factory_addr,
    token::token_init,
};

use crate::prelude::*;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, QueryResponse, Reply, Response};
use cw2::{get_contract_version, set_contract_version};
use perpswap::{
    contracts::market::{
        deferred_execution::DeferredExecId,
        entry::{
            DeltaNeutralityFeeResp, InitialPrice, InstantiateMsg, MigrateMsg, OraclePriceResp,
            PositionsQueryFeeApproach, PriceWouldTriggerResp, SpotPriceHistoryResp,
        },
        position::{PositionOrPendingClose, PositionsResp},
        spot_price::{SpotPriceConfig, SpotPriceConfigInit},
    },
    shutdown::ShutdownImpact,
};

use perpswap::contracts::market::entry::LimitOrderResp;

use perpswap::price::Price;
use semver::Version;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:market";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MARKET_NOT_OPERATIONAL: &str = "market is no longer operational";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    InstantiateMsg {
        factory,
        config,
        market_id,
        token,
        initial_borrow_fee_rate,
        spot_price,
        initial_price,
    }: InstantiateMsg,
) -> Result<Response> {
    // Validate initial price
    match (&spot_price, &initial_price) {
        (SpotPriceConfigInit::Manual { .. }, Some(_)) => (),
        (SpotPriceConfigInit::Manual { .. }, None) => {
            anyhow::bail!("Maual price config used, but no initial price set")
        }
        (SpotPriceConfigInit::Oracle { .. }, None) => (),
        (SpotPriceConfigInit::Oracle { .. }, Some(_)) => {
            anyhow::bail!("Cannot set initial price for oracle price updates")
        }
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    set_factory_addr(deps.storage, &factory.validate(deps.api)?)?;
    config_init(deps.api, deps.storage, config, spot_price)?;
    meta_init(deps.storage, &env, &market_id)?;

    token_init(deps.storage, &deps.querier, token)?;
    fees_init(deps.storage)?;
    liquidity_init(deps.storage)?;
    crank_init(deps.storage)?;
    positions_init(deps.storage)?;
    yield_init(deps.storage)?;

    let (state, mut ctx) = StateContext::new(deps, env)?;
    state.initialize_borrow_fee_rate(&mut ctx, initial_borrow_fee_rate)?;
    state.initialize_funding_totals(&mut ctx)?;

    if let Some(InitialPrice { price, price_usd }) = initial_price {
        state.save_manual_spot_price(&mut ctx, price, price_usd)?;
        state.spot_price_append(&mut ctx)?;
    }

    ctx.into_response(&state)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response> {
    let (mut state, ctx) = StateContext::new(deps, env)?;
    #[cfg(feature = "sanity")]
    state.sanity_check(ctx.storage);

    match msg {
        SudoMsg::ConfigUpdate { update } => {
            update_config(&mut state.config, state.api, ctx.storage, *update)?;
        }
    }

    #[cfg(feature = "sanity")]
    crate::state::sanity::sanity_check_post_execute(
        &state,
        ctx.storage,
        &state.env,
        &state.querier,
        &ctx.fund_transfers,
    );

    ctx.into_response(&state)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;
    #[cfg(feature = "sanity")]
    state.sanity_check(ctx.storage);

    // Semi-parse the message to determine the inner message/sender (relevant
    // for CW20s) and any collateral sent into the contract
    let info = state.parse_perps_message_info(ctx.storage, info, msg)?;

    // Ensure we're not shut down from this action
    if let Some(impact) = ShutdownImpact::for_market_execute_msg(&info.msg) {
        state.ensure_not_shut_down(impact)?;
    }
    state.ensure_not_resetting_lps(&mut ctx, &info.msg)?;

    match info.msg {
        ExecuteMsg::Owner(owner_msg) => {
            state.assert_auth(&info.sender, AuthCheck::Owner)?;

            match owner_msg {
                ExecuteOwnerMsg::ConfigUpdate { update } => {
                    let _ = update;
                    anyhow::bail!(MARKET_NOT_OPERATIONAL);
                }
            }
        }

        ExecuteMsg::SetManualPrice {
            price: _,
            price_usd: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        // cw20
        ExecuteMsg::Receive {
            amount: _,
            msg: _,
            sender: _,
        } => anyhow::bail!("Cannot nest a Receive inside another Receive"),

        ExecuteMsg::OpenPosition {
            slippage_assert: _,
            leverage: _,
            direction: _,
            stop_loss_override: _,
            take_profit: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { id: _ } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }
        ExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage { id, amount } => {
            let _ = (id, amount);
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::UpdatePositionAddCollateralImpactSize {
            id: _,
            slippage_assert: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }
        ExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
            id,
            slippage_assert,
            amount,
        } => {
            let _ = (id, slippage_assert, amount);
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::UpdatePositionLeverage {
            id: _,
            leverage: _,
            slippage_assert: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::UpdatePositionMaxGains {
            id: _,
            max_gains: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::UpdatePositionTakeProfitPrice { id: _, price: _ } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::UpdatePositionStopLossPrice {
            id: _,
            stop_loss: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        #[allow(deprecated)]
        ExecuteMsg::SetTriggerOrder {
            id: _,
            stop_loss_override: _,
            take_profit: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::PlaceLimitOrder {
            trigger_price: _,
            leverage: _,
            direction: _,
            stop_loss_override: _,
            take_profit: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::CancelLimitOrder { order_id } => {
            let _ = order_id;
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::ClosePosition {
            id,
            slippage_assert,
        } => {
            let _ = (id, slippage_assert);
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::Crank { execs, rewards } => {
            let _ = (execs, rewards);
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::ForceWithdrawAll { limit } => {
            state.force_withdraw_all(&mut ctx, limit.unwrap_or(state.config.crank_execs))?;
        }

        ExecuteMsg::DepositLiquidity { stake_to_xlp: _ } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::ReinvestYield {
            stake_to_xlp: _,
            amount: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::WithdrawLiquidity {
            lp_amount,
            claim_yield,
        } => {
            let _ = (lp_amount, claim_yield);
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::ClaimYield {} => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::StakeLp { amount: _ } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::UnstakeXlp { amount } => {
            let _ = amount;
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::StopUnstakingXlp {} => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::CollectUnstakedLp {} => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::NftProxy { sender, msg } => {
            let _ = (sender, msg);
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::LiquidityTokenProxy {
            sender: _,
            kind: _,
            msg: _,
        } => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::TransferDaoFees {} => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::CloseAllPositions {} => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::ProvideCrankFunds {} => {
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }

        ExecuteMsg::PerformDeferredExec {
            id,
            price_point_timestamp,
        } => {
            let _ = (id, price_point_timestamp);
            anyhow::bail!(MARKET_NOT_OPERATIONAL);
        }
    }

    // Make sure either the caller sent no funds into the contract, or whatever
    // funds _were_ sent were used above by a call to info.collateral_sent.take().
    info.funds.ensure_empty()?;

    #[cfg(feature = "sanity")]
    crate::state::sanity::sanity_check_post_execute(
        &state,
        ctx.storage,
        &state.env,
        &state.querier,
        &ctx.fund_transfers,
    );

    ctx.into_response(&state)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    let (state, store) = State::new(deps, env)?;
    #[cfg(feature = "sanity")]
    state.sanity_check(deps.storage);

    match msg {
        QueryMsg::Version {} => get_contract_version(store)?.query_result(),

        QueryMsg::Status { price } => {
            state.override_current_price(store, price)?;
            state.status(store)?.query_result()
        }

        QueryMsg::SpotPrice { timestamp } => match timestamp {
            Some(timestamp) => state.spot_price(store, timestamp),
            None => state.current_spot_price(store),
        }?
        .query_result(),
        QueryMsg::SpotPriceHistory {
            start_after,
            limit,
            order,
        } => {
            let price_points = state.historical_spot_prices(
                store,
                start_after,
                limit.map(|l| l.try_into()).transpose()?,
                order.map(|o| o.into()),
            )?;

            SpotPriceHistoryResp { price_points }.query_result()
        }

        QueryMsg::OraclePrice { validate_age } => match state.config.spot_price.clone() {
            SpotPriceConfig::Manual { .. } => {
                bail!("there is no oracle for this market, it uses a manual price instead");
            }
            SpotPriceConfig::Oracle {
                feeds, feeds_usd, ..
            } => {
                let oracle_price = state.get_oracle_price(validate_age)?;
                let market_id = state.market_id(store)?;
                let price_storage =
                    oracle_price.compose_price(market_id, &feeds, &feeds_usd, state.now())?;

                let block_time = state.now();
                let oracle_publish_time = oracle_price
                    .calculate_publish_time(
                        if validate_age {
                            state.config_volatile_time()
                        } else {
                            u32::MAX
                        },
                        block_time,
                    )?
                    .context("couldn't get an oracle price (no-volatile)")?;

                let price_point =
                    state.make_price_point(store, oracle_publish_time, price_storage)?;
                OraclePriceResp {
                    pyth: oracle_price.pyth,
                    sei: oracle_price.sei,
                    rujira: oracle_price.rujira,
                    stride: oracle_price.stride,
                    simple: oracle_price
                        .simple
                        .into_iter()
                        .map(|(key, value)| (key.into(), value))
                        .collect(),
                    composed_price: price_point,
                }
                .query_result()
            }
        },

        QueryMsg::Positions {
            position_ids,
            skip_calc_pending_fees,
            fees,
            price,
        } => {
            state.override_current_price(store, price)?;

            let mut closed = vec![];
            let mut positions = vec![];
            let mut pending_close = vec![];

            let fees = fees.unwrap_or_else(|| {
                if skip_calc_pending_fees.unwrap_or(false) {
                    PositionsQueryFeeApproach::NoFees
                } else {
                    PositionsQueryFeeApproach::AllFees
                }
            });

            for id in position_ids {
                if let Some(pos) = state.load_closed_position(store, id)? {
                    closed.push(pos);
                } else {
                    let pos = get_position(store, id)?;
                    match state.pos_snapshot_for_open(store, pos, fees)? {
                        PositionOrPendingClose::Open(pos) => positions.push(*pos),
                        PositionOrPendingClose::PendingClose(pending) => {
                            pending_close.push(*pending)
                        }
                    }
                }
            }

            PositionsResp {
                positions,
                pending_close,
                closed,
            }
            .query_result()
        }

        QueryMsg::LimitOrder { order_id } => {
            let order = state.limit_order_load(store, order_id)?;
            let market_type = state.market_type(store)?;

            #[allow(deprecated)]
            LimitOrderResp {
                order_id,
                trigger_price: order.trigger_price,
                collateral: order.collateral,
                leverage: order.leverage,
                direction: order.direction.into_base(market_type),
                max_gains: order.max_gains,
                stop_loss_override: order.stop_loss_override,
                take_profit: backwards_compat_limit_order_take_profit(&state, store, &order)?,
            }
            .query_result()
        }

        QueryMsg::LimitOrders {
            owner,
            start_after,
            limit,
            order,
        } => {
            let owner = owner.validate(deps.api)?;
            state
                .limit_order_load_by_addr(
                    store,
                    owner,
                    start_after,
                    limit,
                    order.map(|x| x.into()),
                )?
                .query_result()
        }

        QueryMsg::ClosedPositionHistory {
            owner,
            cursor,
            limit,
            order,
        } => state
            .closed_positions_history(store, owner.validate(state.api)?, cursor, order, limit)?
            .query_result(),

        QueryMsg::NftProxy { nft_msg } => state.nft_handle_query(store, nft_msg),
        QueryMsg::LiquidityTokenProxy { kind, msg } => {
            state.liquidity_token_handle_query(store, kind, msg)
        }
        QueryMsg::TradeHistorySummary { addr } => state
            .trade_history_get_summary(store, &addr.validate(state.api)?)?
            .query_result(),

        QueryMsg::PositionActionHistory {
            id,
            start_after,
            limit,
            order,
        } => state
            .position_action_get_history(
                store,
                id,
                start_after.map(|x| x.parse()).transpose()?,
                limit,
                order.map(|x| x.into()),
            )?
            .query_result(),

        QueryMsg::TraderActionHistory {
            owner,
            start_after,
            limit,
            order,
        } => state
            .trader_action_get_history(
                store,
                &owner.validate(state.api)?,
                start_after.map(|x| x.parse()).transpose()?,
                limit,
                order.map(|x| x.into()),
            )?
            .query_result(),

        QueryMsg::LpActionHistory {
            addr,
            start_after,
            limit,
            order,
        } => state
            .lp_action_get_history(
                store,
                &addr.validate(state.api)?,
                start_after.map(|x| x.parse()).transpose()?,
                limit,
                order.map(|x| x.into()),
            )?
            .query_result(),

        QueryMsg::LimitOrderHistory {
            addr,
            start_after,
            limit,
            order,
        } => state
            .limit_order_get_history(
                store,
                &addr.validate(state.api)?,
                start_after.map(|x| x.parse()).transpose()?,
                limit,
                order.map(|x| x.into()),
            )?
            .query_result(),

        QueryMsg::LpInfo { liquidity_provider } => state
            .lp_info(store, &liquidity_provider.validate(state.api)?)?
            .query_result(),

        QueryMsg::ReferralStats { addr } => state
            .referral_stats(store, &addr.validate(state.api)?)?
            .query_result(),

        QueryMsg::DeltaNeutralityFee {
            notional_delta,
            pos_delta_neutrality_fee_margin,
        } => {
            let price = state.current_spot_price(store)?;
            let fees = state.calc_delta_neutrality_fee(
                store,
                notional_delta,
                &price,
                pos_delta_neutrality_fee_margin,
            )?;
            let fee_rate = (fees.into_number() / notional_delta.into_number())?;
            let price = (price.price_notional.into_number() * (Number::ONE + fee_rate)?)?;
            DeltaNeutralityFeeResp {
                amount: fees,
                fund_total: DELTA_NEUTRALITY_FUND.may_load(store)?.unwrap_or_default(),
                slippage_assert_price: Price::try_from_number(price)?
                    .into_base_price(state.market_id(store)?.get_market_type()),
            }
            .query_result()
        }
        QueryMsg::PriceWouldTrigger { price } => {
            let would_trigger = state.price_would_trigger(store, price)?;
            PriceWouldTriggerResp { would_trigger }.query_result()
        }
        QueryMsg::ListDeferredExecs {
            addr,
            start_after,
            limit,
        } => {
            let addr = addr.validate(state.api)?;
            state
                .list_deferred_execs(store, addr, start_after, limit)?
                .query_result()
        }

        QueryMsg::GetDeferredExec { id } => state.get_deferred_exec_resp(store, id)?.query_result(),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, MigrateMsg {}: MigrateMsg) -> Result<Response> {
    let (state, ctx) = StateContext::new(deps, env)?;

    #[cfg(feature = "sanity")]
    state.sanity_check(ctx.storage);

    // Make sure we don't have any pre-deferred-execution unpending items.
    state.ensure_liquidation_prices_pending_empty(ctx.storage)?;

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
        let response = Response::new()
            .add_attribute("old_contract_name", old_cw2.contract)
            .add_attribute("old_contract_version", old_cw2.version)
            .add_attribute("new_contract_name", CONTRACT_NAME)
            .add_attribute("new_contract_version", CONTRACT_VERSION);
        Ok(response)
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let deferred_exec_id = DeferredExecId::from_u64(msg.id);
    let (state, mut ctx) = StateContext::new(deps, env)?;
    state.handle_deferred_exec_reply(&mut ctx, deferred_exec_id, msg.result)?;
    state.crank_exec_batch(&mut ctx, None)?;
    ctx.into_response(&state)
}
