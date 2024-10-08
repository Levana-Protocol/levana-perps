use crate::{
    common::{
        get_current_processed_dec_queue_id, get_current_queue_element, get_next_dec_queue_id,
        get_next_inc_queue_id,
    },
    prelude::*,
    types::{
        Commission, DecQueuePosition, HighWaterMark, IncQueuePosition, LeaderComissision,
        LpTokenValue, MarketInfo, MarketWorkInfo, OneLpTokenValue, ProcessingStatus, State,
        WalletInfo,
    },
    work::{get_work, process_queue_item},
};
use anyhow::{bail, Ok};
use msg::contracts::market::{
    entry::{ClosedPositionCursor, ExecuteMsg as MarketExecuteMsg},
    position::ClosedPosition,
};
use msg::contracts::{
    copy_trading,
    market::deferred_execution::{DeferredExecStatus, GetDeferredExecResp},
};
use shared::time::Timestamp;

#[must_use]
enum Funds {
    NoFunds,
    Funds { token: Token, amount: Uint128 },
}

impl Funds {
    #[allow(dead_code)]
    fn require_none(self) -> Result<()> {
        match self {
            Funds::NoFunds => Ok(()),
            Funds::Funds { token, amount } => {
                Err(anyhow!("Unnecessary funds sent: {amount}{token:?}"))
            }
        }
    }

    fn require_token(&self) -> Result<&Token> {
        match self {
            Funds::NoFunds => Err(anyhow!(
                "Message requires attached funds, but none were provided"
            )),
            Funds::Funds { token, .. } => Ok(token),
        }
    }

    fn require_some(self, market_token: &msg::token::Token) -> Result<NonZero<Collateral>> {
        match self {
            Funds::NoFunds => Err(anyhow!(
                "Message requires attached funds, but none were provided"
            )),
            Funds::Funds { token, amount } => {
                token.ensure_matches(market_token)?;
                let collateral = market_token
                    .from_u128(amount.u128())
                    .context("Error converting token amount to Collateral")?;
                NonZero::new(Collateral::from_decimal256(collateral))
                    .context("Impossible 0 collateral provided")
            }
        }
    }
}

struct HandleFunds {
    funds: Funds,
    msg: ExecuteMsg,
    sender: Addr,
}

fn handle_funds(api: &dyn Api, mut info: MessageInfo, msg: ExecuteMsg) -> Result<HandleFunds> {
    match msg {
        ExecuteMsg::Receive {
            sender,
            amount,
            msg,
        } => {
            if info.funds.is_empty() {
                let msg: ExecuteMsg = from_json(msg).context("Invalid msg in CW20 receive")?;
                Ok(HandleFunds {
                    funds: Funds::Funds {
                        token: Token::Cw20(info.sender),
                        amount,
                    },
                    msg,
                    sender: sender
                        .validate(api)
                        .context("Unable to parse CW20 receive message's sender field")?,
                })
            } else {
                Err(anyhow!(
                    "Cannot attach funds when performing a CW20 receive"
                ))
            }
        }
        msg => {
            let funds = match info.funds.pop() {
                None => Funds::NoFunds,
                Some(Coin { denom, amount }) => {
                    ensure!(
                        info.funds.is_empty(),
                        "Multiple funds provided, messages only support one fund denom"
                    );
                    Funds::Funds {
                        token: Token::Native(denom),
                        amount,
                    }
                }
            };
            Ok(HandleFunds {
                funds,
                msg,
                sender: info.sender,
            })
        }
    }
}

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let HandleFunds { funds, msg, sender } = handle_funds(deps.api, info, msg)?;
    let (state, storage) = State::load_mut(deps, &env)?;
    match msg {
        ExecuteMsg::Receive { .. } => Err(anyhow!("Cannot perform a receive within a receive")),
        ExecuteMsg::Deposit {} => {
            let token = funds.require_token()?;
            let market_token = state.get_first_full_token_info(storage, token)?;
            let token = token.clone();
            let funds = funds.require_some(&market_token)?;
            deposit(storage, sender, funds, token)
        }
        ExecuteMsg::Withdraw { shares, token } => {
            funds.require_none()?;
            withdraw(storage, sender, shares, token)
        }
        ExecuteMsg::DoWork {} => {
            funds.require_none()?;
            do_work(state, storage)
        }
        ExecuteMsg::LeaderMsg {
            market_id,
            message,
            collateral,
        } => {
            state.config.ensure_leader(&sender)?;
            funds.require_none()?;
            execute_leader_msg(storage, &state, market_id, message, collateral)
        }
        _ => panic!("Not implemented yet"),
    }
}

#[allow(deprecated)]
fn execute_leader_msg(
    storage: &mut dyn Storage,
    state: &State,
    market_id: MarketId,
    message: Box<MarketExecuteMsg>,
    collateral: Option<NonZero<Collateral>>,
) -> Result<Response> {
    let not_supported_response = |message: &str| {
        let response = Response::new().add_event(
            Event::new("execute-leader-msg")
                .add_attribute("message", message.to_string())
                .add_attribute("unsupported", true.to_string()),
        );
        Ok(response)
    };
    let market_info = crate::state::MARKETS
        .may_load(storage, &market_id)?
        .context("MARKETS store is empty")?;
    let token = state.to_token(&market_info.token)?;
    match *message {
        MarketExecuteMsg::Owner(_) => not_supported_response("owner"),
        // implement
        MarketExecuteMsg::Receive { .. } => todo!(),
        MarketExecuteMsg::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit,
        } => {
            let collateral = match collateral {
                Some(collateral) => collateral,
                None => bail!("No supplied collateral for opening position"),
            };
            if max_gains.is_some() {
                bail!("max_gains is deprecated, use take_profit instead")
            }
            if take_profit.is_none() {
                bail!("take profit is not specified")
            }
            let dec_queue_id = get_next_dec_queue_id(storage)?;
            let leader = state.config.leader.clone();
            crate::state::WALLET_QUEUE_ITEMS.save(
                storage,
                (&leader, QueuePositionId::DecQueuePositionId(dec_queue_id)),
                &(),
            )?;
            let queue_position = DecQueuePosition {
                item: copy_trading::DecQueueItem::MarketItem {
                    id: market_id,
                    token,
                    item: Box::new(DecMarketItem::OpenPosition {
                        collateral,
                        slippage_assert,
                        leverage,
                        direction,
                        stop_loss_override,
                        take_profit,
                    }),
                },
                status: copy_trading::ProcessingStatus::NotProcessed,
                wallet: state.config.leader.clone(),
            };
            crate::state::COLLATERAL_DECREASE_QUEUE.save(
                storage,
                &dec_queue_id,
                &queue_position,
            )?;
            Ok(Response::new().add_event(
                Event::new("open-position")
                    .add_attribute("queue-id", dec_queue_id.to_string())
                    .add_attribute("collateral", collateral.to_string()),
            ))
        }
        // decrea coll
        MarketExecuteMsg::UpdatePositionAddCollateralImpactLeverage { .. } => todo!(),
        // dec collater
        MarketExecuteMsg::UpdatePositionAddCollateralImpactSize { .. } => todo!(),
        // increase coll
        MarketExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage { .. } => todo!(),
        // increas
        MarketExecuteMsg::UpdatePositionRemoveCollateralImpactSize { .. } => todo!(),
        // no impact on collateral. only impatcs notional size.
        MarketExecuteMsg::UpdatePositionLeverage { .. } => todo!(),
        // no impact. todo: look through the codebase.
        MarketExecuteMsg::UpdatePositionMaxGains { .. } => todo!(),
        //
        MarketExecuteMsg::UpdatePositionTakeProfitPrice { .. } => todo!(),
        // no impact
        MarketExecuteMsg::UpdatePositionStopLossPrice { .. } => todo!(),
        // no impact.
        MarketExecuteMsg::SetTriggerOrder { .. } => todo!(),
        // reduces collateral
        MarketExecuteMsg::PlaceLimitOrder { .. } => todo!(),
        // increse collateral
        MarketExecuteMsg::CancelLimitOrder { .. } => todo!(),
        // increase or leave it exactly same.
        MarketExecuteMsg::ClosePosition { .. } => todo!(),
        MarketExecuteMsg::DepositLiquidity { .. } => not_supported_response("deposit-liqudiity"),
        MarketExecuteMsg::ReinvestYield { .. } => not_supported_response("reinvest yield"),
        MarketExecuteMsg::WithdrawLiquidity { .. } => not_supported_response("withdraw-liquidity"),
        MarketExecuteMsg::ClaimYield {} => not_supported_response("claim-yield"),
        MarketExecuteMsg::StakeLp { .. } => not_supported_response("stake-lp"),
        MarketExecuteMsg::UnstakeXlp { .. } => not_supported_response("unstake-xlp"),
        MarketExecuteMsg::StopUnstakingXlp {} => not_supported_response("stop-unstaking-xlp"),
        MarketExecuteMsg::CollectUnstakedLp {} => not_supported_response("collect-unstaked-lp"),
        MarketExecuteMsg::Crank { .. } => not_supported_response("crank"),
        MarketExecuteMsg::NftProxy { .. } => not_supported_response("nft-proxy"),
        MarketExecuteMsg::LiquidityTokenProxy { .. } => {
            not_supported_response("liquidity-token-proxy")
        }
        MarketExecuteMsg::TransferDaoFees {} => not_supported_response("transfer-dao-fees"),
        MarketExecuteMsg::CloseAllPositions {} => not_supported_response("close-all-positions"),
        MarketExecuteMsg::ProvideCrankFunds {} => not_supported_response("provide-crank-funds"),
        MarketExecuteMsg::SetManualPrice { .. } => not_supported_response("set-manual-price"),
        MarketExecuteMsg::PerformDeferredExec { .. } => {
            not_supported_response("perform-deferred-exec")
        }
    }
}

fn withdraw(
    storage: &mut dyn Storage,
    wallet: Addr,
    shares: NonZero<LpToken>,
    token: Token,
) -> Result<Response> {
    let wallet_info = WalletInfo {
        token,
        wallet: wallet.clone(),
    };
    let actual_shares = crate::state::SHARES.may_load(storage, &wallet_info)?;
    let shares = match actual_shares {
        Some(actual_shares) => {
            if shares > actual_shares && shares != actual_shares {
                bail!("Requesting more withdrawal than balance")
            }
            shares
        }
        None => bail!("No shares found"),
    };
    let dec_queue_id = get_next_dec_queue_id(storage)?;
    let queue_id = QueuePositionId::DecQueuePositionId(dec_queue_id);
    crate::state::WALLET_QUEUE_ITEMS.save(storage, (&wallet, queue_id), &())?;
    let queue_position = DecQueuePosition {
        item: copy_trading::DecQueueItem::Withdrawal {
            tokens: shares,
            token: wallet_info.token,
        },
        wallet: wallet_info.wallet,
        status: copy_trading::ProcessingStatus::NotProcessed,
    };
    crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &dec_queue_id, &queue_position)?;
    Ok(Response::new().add_event(
        Event::new("withdrawal")
            .add_attribute("shares", shares.to_string())
            .add_attribute("queue-id", dec_queue_id.to_string()),
    ))
}

fn do_work(state: State, storage: &mut dyn Storage) -> Result<Response> {
    let work = get_work(&state, storage)?;
    let desc = match work {
        WorkResp::NoWork => bail!("No work items available"),
        WorkResp::HasWork { work_description } => work_description,
    };
    let res = Response::new()
        .add_event(Event::new("work-desc").add_attribute("desc", format!("{desc:?}")));

    match desc {
        WorkDescription::LoadMarket {} => {
            state.batched_stored_market_info(storage)?;
            let status = crate::state::MARKET_LOADER_STATUS
                .may_load(storage)?
                .unwrap_or_default();
            let event =
                Event::new("market-loader-status").add_attribute("value", status.to_string());
            let res = res.add_event(event);
            Ok(res)
        }
        WorkDescription::ComputeLpTokenValue { token } => {
            let event = compute_lp_token_value(storage, &state, token)?;
            let res = res.add_event(event);
            Ok(res)
        }
        WorkDescription::ProcessMarket { .. } => todo!(),
        WorkDescription::ProcessQueueItem { id } => {
            let res = process_queue_item(id, storage, &state, res)?;
            Ok(res)
        }
        WorkDescription::ResetStats { token } => reset_stats(storage, &state, token),
        WorkDescription::HandleDeferredExecId {} => {
            let response = handle_deferred_exec_id(storage, &state)?;
            Ok(response)
        }
        WorkDescription::Rebalance { token, amount } => rebalance(storage, &state, token, amount),
    }
}

// Rebalance is done when contract balance is not same as the one
// internally tracked by it. This could occur for a variety of
// reasons like Positions got liquidated, someone sent free money to this
// contract etc.
fn rebalance(
    storage: &mut dyn Storage,
    state: &State,
    token: Token,
    rebalance_amount: NonZero<Collateral>,
) -> Result<Response> {
    let markets = state.load_market_ids_with_token(storage, &token)?;
    let mut totals = crate::state::TOTALS
        .may_load(storage, &token)?
        .unwrap_or_default();
    let rebalance_amount = rebalance_amount.raw();
    let mut check_balance = Collateral::zero();
    let mut rebalanced = false;
    for market in markets {
        if check_balance.approx_eq(rebalance_amount) {
            break;
        }
        let mut cursor = crate::state::LAST_CLOSED_POSITION_CURSOR.may_load(storage, &market.id)?;
        let mut hwm = crate::state::HIGH_WATER_MARK
            .may_load(storage, &token)?
            .unwrap_or_default();
        loop {
            // todo: Batch this operations
            let closed_positions = state.query_closed_position(&market.addr, cursor.clone())?;
            let last_closed_position =
                closed_positions
                    .positions
                    .last()
                    .cloned()
                    .map(|item| ClosedPositionCursor {
                        time: item.close_time,
                        position: item.id,
                    });
            if let Some(ref last_closed_position) = last_closed_position {
                crate::state::LAST_CLOSED_POSITION_CURSOR.save(
                    storage,
                    &market.id,
                    last_closed_position,
                )?
            }
            cursor = last_closed_position;
            for position in closed_positions.positions {
                let commission =
                    handle_leader_commission(storage, state, &token, position, &mut hwm)?;
                check_balance = check_balance.checked_add(commission.active_collateral)?;
                totals.collateral = totals
                    .collateral
                    .checked_add(commission.remaining_collateral)?;
                if commission.profit > Collateral::zero() {
                    // If leader made profit
                    rebalanced = true;
                }
            }
            if closed_positions.cursor.is_none() {
                break;
            }
        }
        crate::state::HIGH_WATER_MARK.save(storage, &token, &hwm)?;
    }
    let mut event = Event::new("rebalanced").add_attribute("made-profit", rebalanced.to_string());
    if check_balance < rebalance_amount {
        // We have settled all the markets's closed positions, but we
        // are still not balanced. This means that the money was sent
        // by someone directly to the contract.
        let diff = rebalance_amount.checked_sub(check_balance)?;
        totals.collateral = totals.collateral.checked_add(diff)?;
        crate::state::TOTALS.save(storage, &token, &totals)?;
        event = event.add_attribute("gains", diff.to_string());
    }
    crate::state::TOTALS.save(storage, &token, &totals)?;
    let response = Response::new().add_event(event);
    Ok(response)
}

fn handle_leader_commission(
    storage: &mut dyn Storage,
    state: &State,
    token: &Token,
    closed_position: ClosedPosition,
    hwm: &mut HighWaterMark,
) -> Result<LeaderComissision> {
    let commission = hwm.add_pnl(
        closed_position.pnl_collateral,
        &state.config.commission_rate,
    )?;
    if commission.0 > Collateral::zero() {
        let leader_comisssion = crate::state::LEADER_COMMISSION
            .may_load(storage, token)?
            .unwrap_or_default();
        let leader_commission = leader_comisssion.checked_add(commission.0)?;
        crate::state::LEADER_COMMISSION.save(storage, token, &leader_commission)?;
        let pnl = closed_position
            .pnl_collateral
            .try_into_non_negative_value()
            .context("Impossible: profit is negative")?;
        let remaining_profit = pnl.checked_sub(commission.0)?;
        let remaining_collateral = closed_position
            .active_collateral
            .checked_sub(commission.0)?;
        Ok(LeaderComissision {
            active_collateral: closed_position.active_collateral,
            profit: pnl,
            commission,
            remaining_profit,
            remaining_collateral,
        })
    } else {
        Ok(LeaderComissision {
            active_collateral: closed_position.active_collateral,
            profit: Collateral::zero(),
            commission: Commission::zero(),
            remaining_profit: Collateral::zero(),
            remaining_collateral: closed_position.active_collateral,
        })
    }
}

fn reset_stats(storage: &mut dyn Storage, state: &State, token: Token) -> Result<Response> {
    let markets = state.load_market_ids_with_token(storage, &token)?;
    let market_work_info = MarketWorkInfo::default();
    for market in markets {
        crate::state::MARKET_WORK_INFO.save(storage, &market.id, &market_work_info)?;
    }
    Ok(Response::new()
        .add_event(Event::new("reset-stats").add_attribute("token", token.to_string())))
}

fn handle_deferred_exec_id(storage: &mut dyn Storage, state: &State) -> Result<Response> {
    let deferred_exec_id = crate::state::REPLY_DEFERRED_EXEC_ID
        .may_load(storage)?
        .flatten();
    let deferred_exec_id = match deferred_exec_id {
        Some(deferred_exec_id) => deferred_exec_id,
        None => bail!("Impossible: Work handle unable to find deferred exec id"),
    };
    let queue_item = get_current_processed_dec_queue_id(storage)?;
    let (queue_id, mut queue_item) = match queue_item {
        Some((queue_id, queue_item)) => (queue_id, queue_item),
        None => bail!("Impossible: Work handle not able to find queue item"),
    };

    assert!(queue_item.status.in_progress());
    let market_id = match queue_item.item.clone() {
        DecQueueItem::MarketItem { id, .. } => id,
        _ => bail!("Impossible: Deferred work handler got non market item"),
    };
    let market_addr = crate::state::MARKETS
        .may_load(storage, &market_id)?
        .context("MARKETS state is empty")?
        .addr;
    let response = state.get_deferred_exec(&market_addr, deferred_exec_id)?;
    let status = match response {
        GetDeferredExecResp::Found { item } => item,
        GetDeferredExecResp::NotFound {} => {
            bail!("Impossible: Deferred exec id not found")
        }
    };
    match status.status {
        DeferredExecStatus::Pending => bail!("Impossible: Deferred exec status is pending"),
        DeferredExecStatus::Success { .. } => {
            queue_item.status = copy_trading::ProcessingStatus::Finished;
            crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &queue_id, &queue_item)?;
            crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &queue_id)?;
            crate::state::REPLY_DEFERRED_EXEC_ID.save(storage, &None)?;
            Ok(Response::new().add_event(
                Event::new("handle-deferred-exec-id").add_attribute("success", true.to_string()),
            ))
        }
        DeferredExecStatus::Failure {
            reason,
            executed,
            crank_price,
        } => {
            queue_item.status =
                copy_trading::ProcessingStatus::Failed(FailedReason::DeferredExecFailure {
                    reason,
                    executed,
                    crank_price,
                });
            crate::state::COLLATERAL_DECREASE_QUEUE.save(storage, &queue_id, &queue_item)?;
            crate::state::LAST_PROCESSED_DEC_QUEUE_ID.save(storage, &queue_id)?;
            crate::state::REPLY_DEFERRED_EXEC_ID.save(storage, &None)?;
            Ok(Response::new().add_event(
                Event::new("handle-deferred-exec-id").add_attribute("success", false.to_string()),
            ))
        }
    }
}

fn deposit(
    storage: &mut dyn Storage,
    sender: Addr,
    funds: NonZero<Collateral>,
    token: Token,
) -> Result<Response> {
    let inc_queue_id = get_next_inc_queue_id(storage)?;
    let queue_id = QueuePositionId::IncQueuePositionId(inc_queue_id);
    crate::state::WALLET_QUEUE_ITEMS.save(storage, (&sender, queue_id), &())?;
    let queue_position = IncQueuePosition {
        item: copy_trading::IncQueueItem::Deposit {
            funds,
            token: token.clone(),
        },
        wallet: sender,
        status: copy_trading::ProcessingStatus::NotProcessed,
    };
    crate::state::COLLATERAL_INCREASE_QUEUE.save(storage, &inc_queue_id, &queue_position)?;
    let mut pending_deposits = crate::state::PENDING_DEPOSITS
        .may_load(storage, &token)
        .context("Could not load TOTALS")?
        .unwrap_or_default();

    pending_deposits = pending_deposits.checked_add(funds.raw())?;
    crate::state::PENDING_DEPOSITS.save(storage, &token, &pending_deposits)?;
    Ok(Response::new().add_event(
        Event::new("deposit")
            .add_attribute("collateral", funds.to_string())
            .add_attribute("queue-id", inc_queue_id.to_string()),
    ))
}

fn compute_lp_token_value(storage: &mut dyn Storage, state: &State, token: Token) -> Result<Event> {
    let token_value = crate::state::LP_TOKEN_VALUE
        .may_load(storage, &token)
        .context("Could not load LP_TOKEN_VALUE")?;
    let queue_id = get_current_queue_element(storage)?;
    let token_value = match token_value {
        Some(token_value) => token_value,
        None => {
            // The value is not yet stored which means no deposit has
            // happened yet. In this case, the initial value of the
            // token would be one.

            let token_value = LpTokenValue {
                value: OneLpTokenValue(Collateral::one()),
                status: crate::types::LpTokenStatus::Valid {
                    timestamp: state.env.block.time.into(),
                    queue_id,
                },
            };
            crate::state::LP_TOKEN_VALUE.save(storage, &token, &token_value)?;
            return Ok(Event::new("lp-token").add_attribute("value", token_value.value.to_string()));
        }
    };

    if token_value.status.valid(&queue_id) {
        return Ok(Event::new("lp-token").add_attribute("value", token_value.value.to_string()));
    }
    // todo: track operations
    let markets = state.load_market_ids_with_token(storage, &token)?;
    for market in &markets {
        process_single_market(storage, state, market)?;
    }
    let mut total_open_position_collateral = Collateral::zero();
    for market in &markets {
        let validation = validate_single_market(storage, state, market)?;
        match validation {
            ValidationStatus::Failed => {
                return Ok(Event::new("lp-token")
                    .add_attribute("validation", "failed".to_string())
                    .add_attribute("market-id", market.id.to_string()));
            }
            ValidationStatus::Success { market } => {
                total_open_position_collateral =
                    total_open_position_collateral.checked_add(market.active_collateral)?;
            }
        }
    }
    let totals = crate::state::TOTALS
        .may_load(storage, &token)
        .context("Could not load TOTALS")?
        .unwrap_or_default();
    let total_collateral = totals
        .collateral
        .checked_add(total_open_position_collateral)?;
    let total_shares = totals.shares;
    let one_share_value = if total_shares.is_zero() || total_collateral.is_zero() {
        Collateral::one()
    } else {
        total_collateral.checked_div_dec(total_shares.into_decimal256())?
    };
    let queue_id = get_current_queue_element(storage)?;
    let token_value = LpTokenValue {
        value: OneLpTokenValue(one_share_value),
        status: crate::types::LpTokenStatus::Valid {
            timestamp: Timestamp::from(state.env.block.time),
            queue_id,
        },
    };

    crate::state::LP_TOKEN_VALUE.save(storage, &token, &token_value)?;
    let event = Event::new("lp-token")
        .add_attribute("validation", "success".to_string())
        .add_attribute("collateral", total_collateral.to_string())
        .add_attribute("shares", total_shares.to_string())
        .add_attribute("value", token_value.value.to_string());
    Ok(event)
}

enum ValidationStatus {
    Failed,
    Success { market: MarketWorkInfo },
}

/// Validates market to check if the total open positions and open
/// limits haven't changed
fn validate_single_market(
    storage: &mut dyn Storage,
    state: &State<'_>,
    market: &MarketInfo,
) -> Result<ValidationStatus> {
    let mut market_work = crate::state::MARKET_WORK_INFO
        .may_load(storage, &market.id)
        .context("Could not load MARKET_WORK_INFO")?
        .unwrap_or_default();
    let mut total_open_positions = 0u64;
    let mut total_orders = 0u64;
    let mut tokens_start_after = None;
    // todo: need to break if query limit exeeded
    loop {
        // We have to iterate again entirely, because a position can
        // close.
        let tokens = state.load_tokens(&market.addr, tokens_start_after)?;
        tokens_start_after = tokens.start_after;
        // todo: optimize if empty tokens
        let positions = state.load_positions(&market.addr, tokens.tokens)?;
        let total_positions = u64::try_from(positions.positions.len())?;
        total_open_positions += total_positions;
        if tokens_start_after.is_none() {
            break;
        }
    }
    if total_open_positions != market_work.count_open_positions {
        market_work.processing_status = ProcessingStatus::ResetRequired;
        crate::state::MARKET_WORK_INFO.save(storage, &market.id, &market_work)?;
        return Ok(ValidationStatus::Failed);
    }
    let mut orders_start_after = None;
    loop {
        let orders = state.load_orders(&market.addr, orders_start_after)?;
        orders_start_after = orders.next_start_after;
        // todo: optimize if empty orders
        total_orders += u64::try_from(orders.orders.len())?;
        if orders_start_after.is_none() {
            break;
        }
    }
    if total_orders != market_work.count_orders {
        market_work.processing_status = ProcessingStatus::ResetRequired;
        crate::state::MARKET_WORK_INFO.save(storage, &market.id, &market_work)?;
        return Ok(ValidationStatus::Failed);
    } else {
        market_work.processing_status = ProcessingStatus::Validated;
    }
    crate::state::MARKET_WORK_INFO.save(storage, &market.id, &market_work)?;
    Ok(ValidationStatus::Success {
        market: market_work,
    })
}

/// Process open positions and orders for a single market.
fn process_single_market(
    storage: &mut dyn Storage,
    state: &State<'_>,
    market: &MarketInfo,
) -> Result<()> {
    // todo: track count of query operations!
    // todo: this needs to be fixed properly when batching is implemented
    // Initialize it to empty before starting
    let mut market_work = MarketWorkInfo::default();
    let mut tokens_start_after = None;
    loop {
        let tokens = state.load_tokens(&market.addr, tokens_start_after)?;
        tokens_start_after = tokens.start_after;
        // todo: optimize if empty tokens
        let positions = state.load_positions(&market.addr, tokens.tokens)?;
        let mut total_collateral = Collateral::zero();
        for position in positions.positions {
            total_collateral = total_collateral.checked_add(position.active_collateral.raw())?;
            market_work.count_open_positions += 1;
        }
        market_work.active_collateral = market_work
            .active_collateral
            .checked_add(total_collateral)?;
        if tokens_start_after.is_none() {
            break;
        }
        // Todo: Also break if query count exeeds!
    }
    // todo: do not save here, if we are saving below
    crate::state::MARKET_WORK_INFO.save(storage, &market.id, &market_work)?;
    let mut orders_start_after = None;
    loop {
        let orders = state.load_orders(&market.addr, orders_start_after)?;
        orders_start_after = orders.next_start_after;
        // todo: optimize if empty orders
        let mut total_collateral = Collateral::zero();
        for order in orders.orders {
            total_collateral = total_collateral.checked_add(order.collateral.raw())?;
            market_work.count_orders += 1;
        }
        market_work.active_collateral = market_work
            .active_collateral
            .checked_add(total_collateral)?;
        if orders_start_after.is_none() {
            break;
        }
        // todo: Also break if query count exceeds
    }
    crate::state::MARKET_WORK_INFO.save(storage, &market.id, &market_work)?;
    Ok(())
}
