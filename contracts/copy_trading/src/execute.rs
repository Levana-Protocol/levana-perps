use crate::{
    common::get_next_queue_id,
    prelude::*,
    types::{
        LpTokenValue, MarketInfo, MarketWorkInfo, OneLpTokenValue, ProcessingStatus, QueuePosition,
        State, WalletInfo,
    },
    work::get_work,
};
use anyhow::bail;
use msg::contracts::copy_trading;
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
            let market_token = state.get_full_token_info(storage, token)?;
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
            do_work(state, storage, &env)
        }
        _ => panic!("Not implemented yet"),
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
    let queue_id = get_next_queue_id(storage)?;
    crate::state::WALLET_QUEUE_ITEMS.save(storage, (&wallet, queue_id), &())?;
    let queue_position = QueuePosition {
        item: copy_trading::QueueItem::Withdrawal {
            tokens: shares,
            token: wallet_info.token,
        },
        wallet: wallet_info.wallet,
    };
    crate::state::PENDING_QUEUE_ITEMS.save(storage, &queue_id, &queue_position)?;
    Ok(Response::new().add_event(
        Event::new("withdrawal")
            .add_attribute("shares", shares.to_string())
            .add_attribute("queue-id", queue_id.to_string()),
    ))
}

fn do_work(state: State, storage: &mut dyn Storage, env: &Env) -> Result<Response> {
    let work = get_work(&state, storage)?;
    let desc = match work {
        WorkResp::NoWork => bail!("No work items available"),
        WorkResp::HasWork { work_description } => work_description,
    };
    let res = Response::new()
        .add_event(Event::new("work-desc").add_attribute("desc", format!("{desc:?}")));

    let (event, msg) = match desc {
        WorkDescription::ComputeLpTokenValue { token } => {
            let event = compute_lp_token_value(storage, &state, token, env)?;
            (event, None)
        }
        WorkDescription::ProcessMarket { .. } => todo!(),
        WorkDescription::ProcessQueueItem { id } => {
            let queue_item = crate::state::PENDING_QUEUE_ITEMS
                .may_load(storage, &id)?
                .context("PENDING_QUEUE_ITEMS load failed")?;
            match queue_item.item {
                QueueItem::Deposit { funds, token } => {
                    let mut totals = crate::state::TOTALS
                        .may_load(storage, &token)
                        .context("Could not load TOTALS")?
                        .unwrap_or_default();
                    let token_value = state.load_lp_token_value(storage, &token)?;
                    let new_shares = totals.add_collateral(funds, token_value)?;
                    crate::state::TOTALS.save(storage, &token, &totals)?;
                    let wallet_info = WalletInfo {
                        token,
                        wallet: queue_item.wallet,
                    };
                    let shares = crate::state::SHARES.may_load(storage, &wallet_info)?;
                    let new_shares = match shares {
                        Some(shares) => shares.checked_add(new_shares.raw())?,
                        None => new_shares,
                    };
                    crate::state::SHARES.save(storage, &wallet_info, &new_shares)?;
                    crate::state::LAST_PROCESSED_QUEUE_ID.save(storage, &id)?;
                    let event = Event::new("deposit")
                        .add_attribute("funds", funds.to_string())
                        .add_attribute("shares", new_shares.to_string());
                    (event, None)
                }
                QueueItem::Withdrawal { tokens, token } => {
                    let shares = tokens;
                    let wallet_info = WalletInfo {
                        token,
                        wallet: queue_item.wallet,
                    };
                    let actual_shares = crate::state::SHARES.may_load(storage, &wallet_info)?;
                    let actual_shares = match actual_shares {
                        Some(actual_shares) => {
                            if shares > actual_shares && shares != actual_shares {
                                bail!("Requesting more withdrawal than balance")
                            }
                            actual_shares
                        }
                        None => bail!("No shares found"),
                    };
                    let token_value = state.load_lp_token_value(storage, &wallet_info.token)?;
                    let funds = token_value.shares_to_collateral(shares)?;
                    let token = state.get_full_token_info(storage, &wallet_info.token)?;
                    let withdraw_msg = token
                        .into_transfer_msg(&wallet_info.wallet, funds)?
                        .context(
                        "Collateral amount would be less than the chain's minimum representation",
                    )?;
                    let remaining_shares = actual_shares.raw().checked_sub(shares.raw())?;
                    if remaining_shares.is_zero() {
                        crate::state::SHARES.remove(storage, &wallet_info);
                    } else {
                        let remaining_shares =
                            NonZero::new(remaining_shares).context("remaining_shares is zero")?;
                        crate::state::SHARES.save(storage, &wallet_info, &remaining_shares)?;
                    }
                    crate::state::LAST_PROCESSED_QUEUE_ID.save(storage, &id)?;
                    let event = Event::new("withdraw")
                        .add_attribute("wallet", wallet_info.wallet.to_string())
                        .add_attribute("burned-shares", shares.to_string());
                    (event, Some(withdraw_msg))
                }
                QueueItem::OpenPosition {} => todo!(),
            }
        }
        WorkDescription::ResetStats {} => todo!(),
        WorkDescription::Rebalance {} => todo!(),
    };
    let response = res.add_event(event);
    let response = match msg {
        Some(msg) => response.add_message(msg),
        None => response,
    };
    Ok(response)
}

fn deposit(
    storage: &mut dyn Storage,
    sender: Addr,
    funds: NonZero<Collateral>,
    token: Token,
) -> Result<Response> {
    let queue_id = get_next_queue_id(storage)?;
    crate::state::WALLET_QUEUE_ITEMS.save(storage, (&sender, queue_id), &())?;
    let queue_position = QueuePosition {
        item: copy_trading::QueueItem::Deposit { funds, token },
        wallet: sender,
    };
    crate::state::PENDING_QUEUE_ITEMS.save(storage, &queue_id, &queue_position)?;
    Ok(Response::new().add_event(
        Event::new("deposit")
            .add_attribute("collateral", funds.to_string())
            .add_attribute("queue-id", queue_id.to_string()),
    ))
}

fn compute_lp_token_value(
    storage: &mut dyn Storage,
    state: &State,
    token: Token,
    env: &Env,
) -> Result<Event> {
    let token_value = crate::state::LP_TOKEN_VALUE
        .may_load(storage, &token)
        .context("Could not load LP_TOKEN_VALUE")?;
    let token_value = match token_value {
        Some(token_value) => token_value,
        None => {
            // The value is not yet stored which means no deposit has
            // happened yet. In this case, the initial value of the
            // token would be one.
            let token_value = LpTokenValue {
                value: OneLpTokenValue(Collateral::one()),
                status: crate::types::LpTokenStatus::Valid {
                    timestamp: env.block.time.into(),
                },
            };
            crate::state::LP_TOKEN_VALUE.save(storage, &token, &token_value)?;
            return Ok(Event::new("lp-token").add_attribute("value", token_value.value.to_string()));
        }
    };

    if token_value.status.valid() {
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
    let one_share_value = total_collateral.checked_div_dec(total_shares.into_decimal256())?;
    let token_value = LpTokenValue {
        value: OneLpTokenValue(one_share_value),
        status: crate::types::LpTokenStatus::Valid {
            timestamp: Timestamp::from(env.block.time),
        },
    };
    crate::state::LP_TOKEN_VALUE.save(storage, &token, &token_value)?;
    let event = Event::new("lp-token")
        .add_attribute("validation", "success".to_string())
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
    // todo: need to break if query limit exeeded
    loop {
        // We have to iterate again entirely, because a position can
        // close.
        let mut tokens_start_after = None;
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
    loop {
        let mut orders_start_after = None;
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
    let mut market_work = crate::state::MARKET_WORK_INFO
        .may_load(storage, &market.id)
        .context("Could not load MARKET_WORK_INFO")?
        .unwrap_or_default();
    loop {
        let mut tokens_start_after = None;
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
    loop {
        let mut orders_start_after = None;
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
