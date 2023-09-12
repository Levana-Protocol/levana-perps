mod perps_info;

use crate::state::{
    config::{config_init, update_config},
    crank::crank_init,
    delta_neutrality_fee::DELTA_NEUTRALITY_FUND,
    fees::fees_init,
    liquidity::{liquidity_init, yield_init},
    meta::meta_init,
    position::{get_position, positions_init, PositionOrId},
    set_factory_addr,
    token::token_init,
};

use crate::prelude::*;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Addr, Deps, DepsMut, Env, MessageInfo, QueryResponse, Response};
use cw2::{get_contract_version, set_contract_version};
use msg::{
    contracts::market::{
        entry::{
            DeltaNeutralityFeeResp, InstantiateMsg, MigrateMsg, PositionsQueryFeeApproach,
            PriceWouldTriggerResp, SpotPriceHistoryResp,
        },
        position::{events::PositionSaveReason, PositionId, PositionOrPendingClose, PositionsResp},
    },
    shutdown::ShutdownImpact,
};

use msg::contracts::market::entry::{LimitOrderResp, SlippageAssert};

use semver::Version;
use shared::price::Price;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:market";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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
    }: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    set_factory_addr(deps.storage, &factory.validate(deps.api)?)?;
    config_init(deps.api, deps.storage, config, spot_price)?;
    meta_init(deps.storage, &market_id)?;

    token_init(deps.storage, &deps.querier, token)?;
    fees_init(deps.storage)?;
    liquidity_init(deps.storage)?;
    crank_init(deps.storage, &env)?;
    positions_init(deps.storage)?;
    yield_init(deps.storage)?;

    let (state, mut ctx) = StateContext::new(deps, env)?;
    state.initialize_borrow_fee_rate(&mut ctx, initial_borrow_fee_rate)?;
    state.initialize_funding_totals(&mut ctx)?;

    ctx.into_response(&state)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (mut state, mut ctx) = StateContext::new(deps, env)?;
    #[cfg(feature = "sanity")]
    state.sanity_check(ctx.storage);

    // update borrow fee rate gradually
    state
        .accumulate_borrow_fee_rate(&mut ctx, state.now())
        .map_err(|e| anyhow::anyhow!("accumulate_borrow_fee_rate failed: {e:?}"))?;

    fn append_spot_price(
        state: &mut State,
        ctx: &mut StateContext,
        rewards_addr: &Addr,
    ) -> Result<()> {
        state.spot_price_append(ctx)?;

        // tests were setup depending on this logic
        state.crank_exec_batch(ctx, Some(0), rewards_addr)?;
        state.crank_current_price_complete(ctx)?;
        Ok(())
    }

    fn handle_update_position_shared(
        state: &State,
        ctx: &mut StateContext,
        sender: Addr,
        id: PositionId,
        notional_size: Option<Signed<Notional>>,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<()> {
        state.ensure_not_stale(ctx.storage)?;

        state.position_assert_owner(ctx.storage, PositionOrId::Id(id), &sender)?;

        if let Some(slippage_assert) = slippage_assert {
            let market_type = state.market_id(ctx.storage)?.get_market_type();
            let pos = get_position(ctx.storage, id)?;
            let delta_notional_size =
                notional_size.unwrap_or(pos.notional_size) - pos.notional_size;
            state.do_slippage_assert(
                ctx.storage,
                slippage_assert,
                delta_notional_size,
                market_type,
                Some(pos.liquidation_margin.delta_neutrality),
            )?;
        }

        let now = state.now();
        let pos = get_position(ctx.storage, id)?;

        let starts_at = pos.liquifunded_at;
        state.position_liquifund_store(
            ctx,
            pos,
            starts_at,
            now,
            false,
            PositionSaveReason::Update,
        )?;

        Ok(())
    }

    // Semi-parse the message to determine the inner message/sender (relevant
    // for CW20s) and any collateral sent into the contract
    let mut info = state.parse_perps_message_info(ctx.storage, info, msg)?;

    // Ensure we're not shut down from this action
    if let Some(impact) = ShutdownImpact::for_market_execute_msg(&info.msg) {
        state.ensure_not_shut_down(impact)?;
    }
    state.ensure_not_resetting_lps(&mut ctx, &info.msg)?;

    if info.requires_spot_price_append {
        append_spot_price(&mut state, &mut ctx, &info.sender)?;
    }

    match info.msg {
        ExecuteMsg::Owner(owner_msg) => {
            state.assert_auth(&info.sender, AuthCheck::Owner)?;

            match owner_msg {
                ExecuteOwnerMsg::ConfigUpdate { update } => {
                    update_config(&mut state.config, ctx.storage, *update)?;
                }
                ExecuteOwnerMsg::SetManualPrice { price, price_usd } => {
                    state.save_manual_spot_price(&mut ctx, price, price_usd)?;
                    // the price needed to be set first before doing this
                    // so info.requires_spot_price_append is false
                    append_spot_price(&mut state, &mut ctx, &info.sender)?;
                }
            }
        }

        // cw20
        ExecuteMsg::Receive {
            amount: _,
            msg: _,
            sender: _,
        } => anyhow::bail!("Cannot nest a Receive inside another Receive"),

        ExecuteMsg::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
        } => {
            state.handle_position_open(
                &mut ctx,
                info.sender,
                info.funds.take()?,
                leverage,
                direction,
                max_gains,
                slippage_assert,
                stop_loss_override,
                take_profit_override,
            )?;
        }

        ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { id } => {
            handle_update_position_shared(&state, &mut ctx, info.sender, id, None, None)?;
            state.update_position_collateral(&mut ctx, id, info.funds.take()?.into_signed())?;
        }
        ExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage { id, amount } => {
            state.get_token(ctx.storage)?.validate_collateral(amount)?;
            handle_update_position_shared(&state, &mut ctx, info.sender, id, None, None)?;
            state.update_position_collateral(&mut ctx, id, -amount.into_signed())?;
        }

        ExecuteMsg::UpdatePositionAddCollateralImpactSize {
            id,
            slippage_assert,
        } => {
            let funds = info.funds.take()?.into_signed();
            let notional_size = state.update_size_new_notional_size(&mut ctx, id, funds)?;
            handle_update_position_shared(
                &state,
                &mut ctx,
                info.sender,
                id,
                Some(notional_size),
                slippage_assert,
            )?;
            state.update_position_size(&mut ctx, id, funds)?;
        }
        ExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
            id,
            slippage_assert,
            amount,
        } => {
            state.get_token(ctx.storage)?.validate_collateral(amount)?;

            let notional_size =
                state.update_size_new_notional_size(&mut ctx, id, -amount.into_signed())?;
            handle_update_position_shared(
                &state,
                &mut ctx,
                info.sender,
                id,
                Some(notional_size),
                slippage_assert,
            )?;
            state.update_position_size(&mut ctx, id, -amount.into_signed())?;
        }

        ExecuteMsg::UpdatePositionLeverage {
            id,
            leverage,
            slippage_assert,
        } => {
            handle_update_position_shared(&state, &mut ctx, info.sender, id, None, None)?;

            let notional_size = state.update_leverage_new_notional_size(&mut ctx, id, leverage)?;
            if let Some(slippage_assert) = slippage_assert {
                let market_type = state.market_id(ctx.storage)?.get_market_type();
                let pos = get_position(ctx.storage, id)?;
                let delta_notional_size = notional_size - pos.notional_size;
                state.do_slippage_assert(
                    ctx.storage,
                    slippage_assert,
                    delta_notional_size,
                    market_type,
                    Some(pos.liquidation_margin.delta_neutrality),
                )?;
            }

            state.update_position_leverage(&mut ctx, id, notional_size)?;
        }

        ExecuteMsg::UpdatePositionMaxGains { id, max_gains } => {
            handle_update_position_shared(&state, &mut ctx, info.sender, id, None, None)?;
            let counter_collateral =
                state.update_max_gains_new_counter_collateral(&mut ctx, id, max_gains)?;
            state.update_position_max_gains(&mut ctx, id, counter_collateral)?;
        }

        ExecuteMsg::SetTriggerOrder {
            id,
            stop_loss_override,
            take_profit_override,
        } => {
            state.position_assert_owner(ctx.storage, PositionOrId::Id(id), &info.sender)?;
            state.set_trigger_order(&mut ctx, id, stop_loss_override, take_profit_override)?;
        }

        ExecuteMsg::PlaceLimitOrder {
            trigger_price,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
        } => {
            let market_type = state.market_id(ctx.storage)?.get_market_type();

            state.limit_order_set_order(
                &mut ctx,
                info.sender,
                trigger_price,
                info.funds.take()?,
                leverage,
                direction.into_notional(market_type),
                max_gains,
                stop_loss_override,
                take_profit_override,
            )?;
        }

        ExecuteMsg::CancelLimitOrder { order_id } => {
            state.limit_order_assert_owner(ctx.storage, &info.sender, order_id)?;
            state.limit_order_cancel_order(&mut ctx, order_id)?;
        }

        ExecuteMsg::ClosePosition {
            id,
            slippage_assert,
        } => {
            state.ensure_not_stale(ctx.storage)?;

            let pos = get_position(ctx.storage, id)?;

            if let Some(slippage_assert) = slippage_assert {
                let market_type = state.market_id(ctx.storage)?.get_market_type();
                state.do_slippage_assert(
                    ctx.storage,
                    slippage_assert,
                    -pos.notional_size,
                    market_type,
                    Some(pos.liquidation_margin.delta_neutrality),
                )?;
            }

            state.position_assert_owner(
                ctx.storage,
                PositionOrId::Pos(Box::new(pos.clone())),
                &info.sender,
            )?;

            state.close_position_via_msg(&mut ctx, pos)?;
        }

        ExecuteMsg::Crank { execs, rewards } => {
            let rewards = match rewards {
                None => info.sender,
                Some(rewards) => rewards.validate(state.api)?,
            };
            state.crank_exec_batch(&mut ctx, execs, &rewards)?;
        }

        ExecuteMsg::DepositLiquidity { stake_to_xlp } => {
            state.liquidity_deposit(&mut ctx, &info.sender, info.funds.take()?, stake_to_xlp)?;
        }

        ExecuteMsg::ReinvestYield {
            stake_to_xlp,
            amount,
        } => {
            state.reinvest_yield(&mut ctx, &info.sender, amount, stake_to_xlp)?;
        }

        ExecuteMsg::WithdrawLiquidity { lp_amount } => {
            state.liquidity_withdraw(&mut ctx, &info.sender, lp_amount)?;
        }

        ExecuteMsg::ClaimYield {} => {
            state.liquidity_claim_yield(&mut ctx, &info.sender, true)?;
        }

        ExecuteMsg::StakeLp { amount } => {
            state.liquidity_stake_lp(&mut ctx, &info.sender, amount)?;
        }

        ExecuteMsg::UnstakeXlp { amount } => {
            state.liquidity_unstake_xlp(&mut ctx, &info.sender, amount)?
        }

        ExecuteMsg::StopUnstakingXlp {} => {
            state.liquidity_stop_unstaking_xlp(&mut ctx, &info.sender, true, true)?;
        }

        ExecuteMsg::CollectUnstakedLp {} => {
            let collected = state.collect_unstaked_lp(&mut ctx, &info.sender)?;
            if !collected {
                bail!("There is no unstaked LP for {}", info.sender)
            }
        }

        ExecuteMsg::NftProxy { sender, msg } => {
            let position_token_addr = state.position_token_addr(ctx.storage)?;
            // executions *MUST* come only from the proxy contract
            // otherwise anyone could spoof the sender
            state.assert_auth(&info.sender, AuthCheck::Addr(position_token_addr))?;
            state.nft_handle_exec(&mut ctx, sender.validate(state.api)?, msg)?;
        }

        ExecuteMsg::LiquidityTokenProxy { sender, kind, msg } => {
            let liquidity_token_addr = state.liquidity_token_addr(ctx.storage, kind)?;
            // executions *MUST* come only from the proxy contract
            // otherwise anyone could spoof the sender
            state.assert_auth(&info.sender, AuthCheck::Addr(liquidity_token_addr))?;

            state.liquidity_token_handle_exec(&mut ctx, sender.validate(state.api)?, kind, msg)?;
        }

        ExecuteMsg::TransferDaoFees {} => {
            state.transfer_fees_to_dao(&mut ctx)?;
        }

        ExecuteMsg::CloseAllPositions {} => {
            state.assert_auth(&info.sender, AuthCheck::WindDown)?;
            state.set_close_all_positions(&mut ctx)?;
        }

        ExecuteMsg::ProvideCrankFunds {} => {
            state.provide_crank_funds(&mut ctx, info.funds.take()?)?;
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

        QueryMsg::SpotPrice { timestamp } => state.spot_price(store, timestamp)?.query_result(),
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

            LimitOrderResp {
                order_id,
                trigger_price: order.trigger_price,
                collateral: order.collateral,
                leverage: order.leverage,
                direction: order.direction.into_base(market_type),
                max_gains: order.max_gains,
                stop_loss_override: order.stop_loss_override,
                take_profit_override: order.take_profit_override,
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

        QueryMsg::DeltaNeutralityFee {
            notional_delta,
            pos_delta_neutrality_fee_margin,
        } => {
            let price = state.spot_price(store, None)?;
            let fees = state.calc_delta_neutrality_fee(
                store,
                notional_delta,
                price,
                pos_delta_neutrality_fee_margin,
            )?;
            let fee_rate = fees.into_number() / notional_delta.into_number();
            let price = price.price_notional.into_number() * (Number::ONE + fee_rate);
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
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, _msg: MigrateMsg) -> Result<Response> {
    let (_state, ctx) = StateContext::new(deps, env)?;

    // Note, we use _state instead of state to avoid warnings when compiling without the sanity
    // feature

    #[cfg(feature = "sanity")]
    _state.sanity_check(ctx.storage);

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
