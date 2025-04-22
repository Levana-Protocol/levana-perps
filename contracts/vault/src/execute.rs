use perpswap::{
    contracts::vault::ExecuteMsg,
    number::{LpToken, NonZero},
    storage::MarketExecuteMsg,
};

use crate::{
    common::{check_not_paused, get_and_increment_queue_id, get_total_assets},
    prelude::*,
    state::{self, QueueId, LP_BALANCES},
    types::WithdrawalRequest,
};

use std::collections::HashMap;

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    match msg {
        ExecuteMsg::Deposit {} => execute_deposit(deps, env, info),

        ExecuteMsg::RequestWithdrawal { amount } => execute_request_withdrawal(deps, info, amount),

        ExecuteMsg::RedistributeFunds {} => execute_redistribute_funds(deps, env, info),

        ExecuteMsg::CollectYield {} => execute_collect_yield(deps, info),

        ExecuteMsg::ProcessWithdrawal {} => execute_process_withdrawal(deps, env, info),

        ExecuteMsg::WithdrawFromMarket { market, amount } => {
            execute_withdraw_from_market(deps, info, market, amount)
        }

        ExecuteMsg::EmergencyPause {} => execute_emergency_pause(deps, info),

        ExecuteMsg::ResumeOperations {} => execute_resume_operations(deps, info),

        ExecuteMsg::UpdateAllocations { new_allocations } => {
            execute_update_allocations(deps, info, new_allocations)
        }

        ExecuteMsg::AddMarket { market } => execute_add_market(deps, info, market),
    }
}

fn execute_deposit(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response> {
    let config = state::CONFIG.load(deps.storage)?;
    check_not_paused(&config)?;
    let sender = info.sender.clone();

    if info.funds.len() != 1 || info.funds[0].denom != config.usdc_denom {
        return Err(anyhow!("Exactly one coin (USDC) must be sent",));
    }
    let amount = info.funds[0].amount;

    // Calculate the amount of USDCLP tokens to mint based on total assets and LP supply
    // Note: This assumes total_assets reflects the net value without impairment.
    // Future complexity (e.g., impairment, fees) may require adjusting total_assets or adding factors.
    let total_assets = get_total_assets(deps.as_ref(), &env)?;
    let total_lp = state::TOTAL_LP_SUPPLY.load(deps.storage)?;
    let lp_amount = if total_lp.is_zero() {
        amount
    } else {
        amount.multiply_ratio(total_lp, total_assets.total_assets)
    };

    // Update user LP balance
    LP_BALANCES.update(deps.storage, &sender, |balance| -> Result<Uint128> {
        Ok(balance.unwrap_or_default() + lp_amount)
    })?;

    state::TOTAL_LP_SUPPLY.update(deps.storage, |t| -> Result<Uint128> { Ok(t + lp_amount) })?;

    Ok(Response::new().add_attributes(vec![
        ("action", "deposit"),
        ("user", sender.as_str()),
        ("amount", &amount.to_string()),
        ("lp_minted", &lp_amount.to_string()),
    ]))
}

fn execute_request_withdrawal(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response> {
    let config = state::CONFIG.load(deps.storage)?;
    check_not_paused(&config)?;
    let sender = info.sender.clone();

    // Create a message to burn the user's USDCLP tokens
    LP_BALANCES.update(deps.storage, &sender, |balance| -> Result<Uint128> {
        balance
            .unwrap_or_default()
            .checked_sub(amount)
            .map_err(|e| anyhow!(format!("Error reducing balances: {}", e)))
    })?;

    let withdrawal_request = WithdrawalRequest {
        user: sender.clone(),
        amount,
    };

    let queue_id = get_and_increment_queue_id(deps.storage)?;

    if withdrawal_request.amount.is_zero() {
        return Err(anyhow!("Withdrawal amount cannot be zero"));
    }

    state::WITHDRAWAL_QUEUE.save(deps.storage, queue_id, &withdrawal_request)?;
    state::USER_WITHDRAWALS.save(deps.storage, (&sender, queue_id), &())?;

    state::TOTAL_PENDING_WITHDRAWALS.update(deps.storage, |t| -> Result<Uint128> {
        t.checked_add(amount)
            .map_err(|e| anyhow!(format!("Error updating total pending withdrawals: {}", e)))
    })?;

    state::TOTAL_LP_SUPPLY.update(deps.storage, |t| -> Result<Uint128> {
        t.checked_sub(amount)
            .map_err(|e| anyhow!(format!("Insufficient LP supply: {}", e)))
    })?;

    Ok(Response::new().add_attributes(vec![
        ("action", "request_withdrawal"),
        ("user", sender.as_str()),
        ("amount", &amount.to_string()),
    ]))
}

fn execute_redistribute_funds(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response> {
    let config = state::CONFIG.load(deps.storage)?;

    check_not_paused(&config)?;
    if config.governance != info.sender {
        return Err(anyhow!("Unauthorized redistribute_funds"));
    }

    let pending = state::TOTAL_PENDING_WITHDRAWALS
        .load(deps.storage)
        .unwrap_or(Uint128::zero());

    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;

    let excess = vault_balance.saturating_sub(pending);
    if excess.is_zero() {
        return Err(anyhow!("No excess to redistribute"));
    }

    let total_bps: u16 = config.markets_allocation_bps.values().sum();
    if total_bps == 0 {
        return Err(anyhow!("No allocation percentages defined"));
    }
    if total_bps > 10_000 {
        return Err(anyhow!("Market allocation exceeds 100%"));
    }

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut remaining = excess;

    for (market, allocation_bps) in config.markets_allocation_bps.iter() {
        let amount = excess.multiply_ratio(*allocation_bps, total_bps);
        if !amount.is_zero() {
            let deposit_msg = WasmMsg::Execute {
                contract_addr: market.to_string(),
                msg: to_json_binary(&MarketExecuteMsg::DepositLiquidity {
                    stake_to_xlp: false,
                })?,
                funds: vec![Coin {
                    denom: config.usdc_denom.clone(),
                    amount,
                }],
            };

            messages.push(deposit_msg.into());

            state::MARKET_ALLOCATIONS.update(
                deps.storage,
                market.as_str(),
                |a| -> Result<Uint128, StdError> { Ok(a.unwrap_or(Uint128::zero()) + amount) },
            )?;

            remaining = remaining.saturating_sub(amount);
        }
    }

    if !remaining.is_zero() {
        messages.push(
            BankMsg::Send {
                to_address: config.governance.to_string(),
                amount: vec![Coin {
                    denom: config.usdc_denom.clone(),
                    amount: remaining,
                }],
            }
            .into(),
        );
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "redistribute_funds"),
        ("excess", &excess.to_string()),
        ("remaining", &remaining.to_string()),
    ]))
}

fn execute_collect_yield(deps: DepsMut, info: MessageInfo) -> Result<Response> {
    let config = state::CONFIG.load(deps.storage)?;

    check_not_paused(&config)?;
    if config.governance != info.sender {
        return Err(anyhow!("Unauthorized"));
    }

    let markets: Vec<String> = state::MARKET_ALLOCATIONS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<String>>>()?;

    let messages: Result<Vec<CosmosMsg>, StdError> = markets
        .iter()
        .map(|market| {
            Ok::<CosmosMsg, StdError>(
                WasmMsg::Execute {
                    contract_addr: market.to_string(),
                    msg: to_json_binary(&MarketExecuteMsg::ClaimYield {})?,
                    funds: vec![],
                }
                .into(),
            )
        })
        .collect();

    Ok(Response::new()
        .add_messages(messages?)
        .add_attribute("action", "collect_yield")
        .add_attribute("markets_processed", markets.len().to_string()))
}

fn execute_process_withdrawal(deps: DepsMut, env: Env, _info: MessageInfo) -> Result<Response> {
    let config = state::CONFIG.load(deps.storage)?;

    if config.paused {
        return Err(anyhow!("The contract is paused"));
    }

    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;

    let mut processed_amount = Uint128::zero();
    let limit = 20;
    let mut messages: Vec<CosmosMsg> = Vec::with_capacity(limit);
    let mut processed_entries: Vec<(QueueId, Addr)> = Vec::with_capacity(limit);

    for item in state::WITHDRAWAL_QUEUE
        .range(deps.storage, None, None, Order::Ascending)
        .take(limit)
    {
        let (id, request) = item?;
        let new_total = processed_amount + request.amount;
        if new_total > vault_balance {
            break;
        }
        processed_amount = new_total;

        messages.push(
            BankMsg::Send {
                to_address: request.user.to_string(),
                amount: vec![Coin {
                    denom: config.usdc_denom.clone(),
                    amount: request.amount,
                }],
            }
            .into(),
        );

        processed_entries.push((id, request.user.clone()));
    }

    for (id, user) in &processed_entries {
        state::WITHDRAWAL_QUEUE.remove(deps.storage, *id);
        state::USER_WITHDRAWALS.remove(deps.storage, (user, *id));
    }

    if !processed_amount.is_zero() {
        let mut total = state::TOTAL_PENDING_WITHDRAWALS.load(deps.storage)?;
        total = total
            .checked_sub(processed_amount)
            .map_err(|e| anyhow!("Error proccesing pending withdrawals: {}", e))?;
        state::TOTAL_PENDING_WITHDRAWALS.save(deps.storage, &total)?;
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "process_withdrawal")
        .add_attribute("processed", processed_entries.len().to_string())
        .add_attribute("amount", processed_amount.to_string()))
}

fn execute_withdraw_from_market(
    deps: DepsMut,
    info: MessageInfo,
    market: String,
    amount: Uint128,
) -> Result<Response> {
    let config = state::CONFIG.load(deps.storage)?;
    check_not_paused(&config)?;
    if config.governance != info.sender {
        return Err(anyhow!("Unauthorized"));
    }

    let current_allocation = state::MARKET_ALLOCATIONS.load(deps.storage, &market)?;
    if current_allocation < amount {
        return Err(anyhow!("Insufficient market allocation"));
    }

    // Convert Uint128 to LpToken and then to NonZero<LpToken>
    let lp_amount = LpToken::from_u128(amount.into()).expect("Can't convert Uint128 to LpToken");
    let lp_amount =
        Some(NonZero::new(lp_amount).ok_or_else(|| anyhow!("Amount must be non-zero"))?);

    // Create a message to withdraw funds from the market
    let withdraw_msg = WasmMsg::Execute {
        contract_addr: market.clone(),
        msg: to_json_binary(&MarketExecuteMsg::WithdrawLiquidity {
            lp_amount,
            claim_yield: false,
        })?,
        funds: vec![],
    };

    // Update the market allocation by subtracting the withdrawn amount
    state::MARKET_ALLOCATIONS.update(deps.storage, &market, |a| -> Result<Uint128, StdError> {
        Ok(a.unwrap_or(Uint128::zero()) - amount)
    })?;

    // Return a response with the withdraw message and attributes
    Ok(Response::new()
        .add_message(withdraw_msg)
        .add_attributes(vec![
            ("action", "withdraw_from_market"),
            ("market", &market),
            ("amount", &amount.to_string()),
        ]))
}

fn execute_emergency_pause(deps: DepsMut, info: MessageInfo) -> Result<Response> {
    let mut config = state::CONFIG.load(deps.storage)?;

    if info.sender != config.governance {
        return Err(anyhow!("Unauthorized"));
    }

    config.paused = true;

    state::CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "emergency_pause"))
}

fn execute_resume_operations(deps: DepsMut, info: MessageInfo) -> Result<Response> {
    let mut config = state::CONFIG.load(deps.storage)?;

    if info.sender != config.governance {
        return Err(anyhow!("Unauthorized"));
    }

    config.paused = false;

    state::CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "resume_operations"))
}

fn execute_update_allocations(
    deps: DepsMut,
    info: MessageInfo,
    new_allocations: HashMap<Addr, u16>,
) -> Result<Response> {
    let mut config = state::CONFIG.load(deps.storage)?;
    if info.sender != config.governance {
        return Err(anyhow!("Unauthorized"));
    }

    let market_count = state::MARKET_ALLOCATIONS
        .keys(deps.storage, None, None, Order::Ascending)
        .count();

    if new_allocations.len() != market_count {
        return Err(anyhow!(format!(
            "Number of allocations ({}) must match number of markets ({})",
            new_allocations.len(),
            market_count
        )));
    }

    let total_bps: u16 = new_allocations.values().sum();
    if total_bps == 0 {
        return Err(anyhow!("No allocation percentages defined"));
    }
    if total_bps > 10_000 {
        return Err(anyhow!("Market allocation exceeds 100%"));
    }

    config.markets_allocation_bps = new_allocations;
    state::CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_allocations"))
}

fn execute_add_market(deps: DepsMut, info: MessageInfo, market: String) -> Result<Response> {
    let config = state::CONFIG.load(deps.storage)?;
    if info.sender != config.governance {
        return Err(anyhow!("Unauthorized"));
    }

    let market_addr = deps.api.addr_validate(&market)?;
    state::MARKET_ALLOCATIONS.save(deps.storage, market_addr.as_str(), &Uint128::zero())?;

    Ok(Response::new()
        .add_attribute("action", "add_market")
        .add_attribute("market", market))
}
