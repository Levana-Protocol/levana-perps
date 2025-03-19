use perpswap::contracts::vault::ExecuteMsg;

use crate::{
    common::{check_not_paused, get_total_assets, is_authorized},
    prelude::*,
    state,
};

// External interfaces (simulated for this example)
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FactoryQueryMsg {
    GetMarkets {
        start_after: Option<String>,
        limit: Option<u32>,
    }, // Query markets from the factory
}

#[derive(Serialize, Deserialize)]
struct GetMarketsResponse {
    markets: Vec<String>, // List of market addresses
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MarketQueryMsg {
    GetUtilization {}, // Query market utilization
}

#[derive(Serialize, Deserialize)]
struct GetUtilizationResponse {
    utilization: Uint128, // Market utilization level
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MarketExecuteMsg {
    Deposit { amount: Uint128 },  // Deposit funds into the market
    ClaimYield {},                // Claim yields from the market
    Withdraw { amount: Uint128 }, // Withdraw funds from the market
}

/// Entry point for executing actions in the contract
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `env`: Contract environment
/// - `info`: Message information
/// - `msg`: Execution message to process
///
/// # Returns
/// - `StdResult<Response>`: Response from the executed action
#[allow(dead_code)]
#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Deposit { amount } => execute_deposit(deps, env, info, amount),

        ExecuteMsg::RequestWithdrawal { amount } => {
            execute_request_withdrawal(deps, env, info, amount)
        }

        ExecuteMsg::RedistributeFunds {} => execute_redistribute_funds(deps, env, info),

        ExecuteMsg::CollectYield { batch_limit } => execute_collect_yield(deps, info, batch_limit),

        ExecuteMsg::ProcessWithdrawal { user } => execute_process_withdrawal(deps, env, info, user),

        ExecuteMsg::WithdrawFromMarket { market, amount } => {
            execute_withdraw_from_market(deps, env, info, market, amount)
        }

        ExecuteMsg::UpdateOperators { add, remove } => {
            execute_update_operators(deps, info, add, remove)
        }

        ExecuteMsg::EmergencyPause {} => execute_emergency_pause(deps, info),

        ExecuteMsg::ResumeOperations {} => execute_resume_operations(deps, info),

        ExecuteMsg::UpdateAllocations { new_allocations } => {
            execute_update_allocations(deps, info, new_allocations)
        }
    }
}

/// Updates the list of operators authorized to manage the vault.
///
/// # Parameters
/// * `deps` - Mutable dependencies providing access to storage and API for address validation.
/// * `info` - Message info containing the sender's address, used to verify governance authority.
/// * `add` - A vector of strings representing addresses to be added as operators.
/// * `remove` - A vector of strings representing addresses to be removed from operators.
///
/// # Returns
/// Returns a `Response` with attributes indicating the action and the number of operators
/// added and removed.
fn execute_update_operators(
    deps: DepsMut,
    info: MessageInfo,
    add: Vec<String>,
    remove: Vec<String>,
) -> StdResult<Response> {
    let mut config = state::CONFIG.load(deps.storage)?;

    // Restrict to governance only
    if info.sender != config.governance {
        return Err(StdError::generic_err(
            "Only governance can update operators",
        ));
    }

    // Validate and add new operators
    let mut operators = config.operators;
    for addr in &add {
        let validated_addr = deps.api.addr_validate(addr)?;
        if !operators.contains(&validated_addr) {
            operators.push(validated_addr);
        }
    }

    // Remove specified operators
    for addr in &remove {
        let validated_addr = deps.api.addr_validate(addr)?;
        operators.retain(|op| op != validated_addr);
    }

    // Update config
    config.operators = operators;
    state::CONFIG.save(deps.storage, &config)?;

    // Return response
    Ok(Response::new()
        .add_attribute("action", "update_operators")
        .add_attribute("added", add.len().to_string())
        .add_attribute("removed", remove.len().to_string()))
}

/// Allows a user to deposit USDC and receive USDCLP tokens
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `env`: Contract environment
/// - `info`: Message information
/// - `amount`: Amount of USDC to deposit
///
/// # Returns
/// - `StdResult<Response>`: Response with mint message and attributes
fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let sender = info.sender.clone(); // Address of the user depositing

    // Verify that USDC was sent and matches the specified amount
    let usdc = info
        .funds
        .iter()
        .find(|c| c.denom == config.usdc_denom)
        .ok_or_else(|| StdError::generic_err("No USDC sent"))?;
    if usdc.amount != amount {
        return Err(StdError::generic_err("Mismatched USDC amount"));
    }

    // Calculate the amount of USDCLP tokens to mint based on total assets and LP supply
    let total_assets = get_total_assets(deps.as_ref(), &env)?;
    let total_lp = state::TOTAL_LP_SUPPLY.load(deps.storage)?;
    let lp_amount = if total_lp.is_zero() {
        amount
    } else {
        amount.multiply_ratio(total_lp, total_assets)
    };

    // Create a message to mint USDCLP tokens
    let mint_msg = WasmMsg::Execute {
        contract_addr: config.usdclp_address.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Mint {
            recipient: sender.to_string(),
            amount: lp_amount,
        })?,
        funds: vec![],
    };

    // Update the total LP token supply
    state::TOTAL_LP_SUPPLY.update(deps.storage, |t| -> Result<Uint128, StdError> {
        Ok(t + lp_amount)
    })?;

    // Return a response with the mint message and attributes
    Ok(Response::new().add_message(mint_msg).add_attributes(vec![
        ("action", "deposit"),
        ("user", sender.as_str()),
        ("amount", &amount.to_string()),
        ("lp_minted", &lp_amount.to_string()),
    ]))
}

/// Registers a withdrawal request by burning USDCLP tokens
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `_env`: Contract environment (unused)
/// - `info`: Message information
/// - `amount`: Amount of USDCLP to burn
///
/// # Returns
/// - `StdResult<Response>`: Response with burn message and attributes
fn execute_request_withdrawal(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let sender = info.sender.clone(); // Address of the user requesting withdrawal

    // Create a message to burn the user's USDCLP tokens
    let burn_msg = WasmMsg::Execute {
        contract_addr: config.usdclp_address.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::BurnFrom {
            owner: sender.to_string(),
            amount,
        })?,
        funds: vec![],
    };

    // Update the pending withdrawal amount and reduce the LP supply
    state::PENDING_WITHDRAWALS.update(
        deps.storage,
        sender.as_str(),
        |p| -> Result<Uint128, StdError> { Ok(p.unwrap_or(Uint128::zero()) + amount) },
    )?;
    state::TOTAL_LP_SUPPLY.update(deps.storage, |t| -> Result<Uint128, StdError> {
        Ok(t - amount)
    })?;

    // Return a response with the burn message and attributes
    Ok(Response::new().add_message(burn_msg).add_attributes(vec![
        ("action", "request_withdrawal"),
        ("user", sender.as_str()),
        ("amount", &amount.to_string()),
    ]))
}

/// Redistributes excess funds to markets based on their utilization
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `env`: Contract environment
/// - `info`: Message information
///
/// # Returns
/// - `StdResult<Response>`: Response with deposit messages and attributes
fn execute_redistribute_funds(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?;
    if !is_authorized(&deps.as_ref(), &info.sender)? {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let config = state::CONFIG.load(deps.storage)?;
    let pending: Uint128 = state::PENDING_WITHDRAWALS
        .range(deps.storage, None, None, Order::Ascending)
        .try_fold(Uint128::zero(), |acc, res| -> Result<Uint128, StdError> {
            Ok(acc + res?.1)
        })?;
    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;
    let excess = vault_balance
        .checked_sub(pending)
        .unwrap_or(Uint128::zero());

    if excess.is_zero() {
        return Err(StdError::generic_err("No excess to redistribute"));
    }

    // Retrieve the list of markets from the factory
    let mut markets = vec![];
    let mut start_after: Option<String> = None;
    loop {
        let batch: GetMarketsResponse =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: config.factory_address.to_string(),
                msg: to_json_binary(&FactoryQueryMsg::GetMarkets {
                    start_after: start_after.clone(),
                    limit: Some(30),
                })?,
            }))?;
        if batch.markets.is_empty() {
            break;
        }
        markets.extend(batch.markets);
        start_after = markets.last().cloned();
    }

    // Get utilization for each market and sort by highest utilization
    let mut utilizations: Vec<(String, Uint128)> = markets
        .into_iter()
        .filter_map(|market| {
            deps.querier
                .query(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: market.clone(),
                    msg: to_json_binary(&MarketQueryMsg::GetUtilization {}).ok()?,
                }))
                .map(|util: GetUtilizationResponse| (market, util.utilization))
                .ok()
        })
        .collect();
    utilizations.sort_by(|a, b| b.1.cmp(&a.1));

    // Distribute funds according to allocation percentages
    let total_bps: u16 = config.yield_allocation_bps.iter().sum();
    let mut messages: Vec<CosmosMsg> = vec![]; // Fixed: Use Vec<CosmosMsg>
    let mut remaining = excess;

    for (i, bps) in config.yield_allocation_bps.iter().enumerate() {
        if let Some((market, _)) = utilizations.get(i) {
            let amount = excess.multiply_ratio(*bps as u128, total_bps as u128);
            if !amount.is_zero() {
                let deposit_msg = WasmMsg::Execute {
                    contract_addr: market.clone(),
                    msg: to_json_binary(&MarketExecuteMsg::Deposit { amount })?,
                    funds: vec![Coin {
                        denom: config.usdc_denom.clone(),
                        amount,
                    }],
                };
                messages.push(deposit_msg.into()); // Convert to CosmosMsg
                state::MARKET_ALLOCATIONS.update(
                    deps.storage,
                    market.as_str(),
                    |a| -> Result<Uint128, StdError> { Ok(a.unwrap_or(Uint128::zero()) + amount) },
                )?;
                remaining = remaining.checked_sub(amount).unwrap_or(Uint128::zero());
            }
        }
    }

    // Send remaining funds to governance
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
        ); // Convert to CosmosMsg
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "redistribute")
        .add_attribute("total_allocated", excess.to_string()))
}

/// Collects yields from markets
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `env`: Contract environment
/// - `info`: Message information
/// - `batch_limit`: Optional limit on the number of markets to process per batch
///
/// # Returns
/// - `StdResult<Response>`: Response with yield collection messages and attributes
fn execute_collect_yield(
    deps: DepsMut,
    info: MessageInfo,
    batch_limit: Option<u32>,
) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
    if !is_authorized(&deps.as_ref(), &info.sender)? {
        // Check authorization
        return Err(StdError::generic_err("Unauthorized"));
    }

    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let limit = batch_limit.unwrap_or(20).min(50); // Batch limit, max 50
    let mut start_after: Option<String> = None;
    let mut messages: Vec<WasmMsg> = vec![];

    // Retrieve markets and create messages to claim yields
    loop {
        let markets: GetMarketsResponse =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: config.factory_address.to_string(),
                msg: to_json_binary(&FactoryQueryMsg::GetMarkets {
                    start_after: start_after.clone(),
                    limit: Some(limit),
                })?,
            }))?;
        if markets.markets.is_empty() {
            break;
        }

        messages.extend(markets.markets.iter().filter_map(|market| {
            Some(WasmMsg::Execute {
                contract_addr: market.clone(),
                msg: to_json_binary(&MarketExecuteMsg::ClaimYield {}).ok()?,
                funds: vec![],
            })
        }));
        start_after = markets.markets.last().cloned();
    }

    // Return a response with messages and attribute
    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "collect_yield"))
}

/// Processes a pending withdrawal by sending USDC to the user
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `env`: Contract environment
/// - `info`: Message information
/// - `user`: Address of the user whose withdrawal is being processed
///
/// # Returns
/// - `StdResult<Response>`: Response with send message and attributes
fn execute_process_withdrawal(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    user: String,
) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
    if !is_authorized(&deps.as_ref(), &info.sender)? {
        // Check authorization
        return Err(StdError::generic_err("Unauthorized"));
    }

    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let user_addr = deps.api.addr_validate(&user)?; // Validate user address
    let amount = state::PENDING_WITHDRAWALS.load(deps.storage, user.as_str())?; // Get pending amount

    // Check if the vault has sufficient balance
    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;
    if vault_balance < amount {
        return Err(StdError::generic_err("Insufficient vault balance"));
    }

    // Remove the pending withdrawal and send USDC to the user
    state::PENDING_WITHDRAWALS.remove(deps.storage, user.as_str());
    let send_msg = BankMsg::Send {
        to_address: user_addr.to_string(),
        amount: vec![Coin {
            denom: config.usdc_denom.clone(),
            amount,
        }],
    };

    // Return a response with the send message and attributes
    Ok(Response::new().add_message(send_msg).add_attributes(vec![
        ("action", "process_withdrawal"),
        ("user", user.as_str()),
        ("amount", &amount.to_string()),
    ]))
}

/// Withdraws funds from a specific market
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `_env`: Contract environment (unused)
/// - `info`: Message information
/// - `market`: Address of the market
/// - `amount`: Amount to withdraw
///
/// # Returns
/// - `StdResult<Response>`: Response with withdraw message and attributes
fn execute_withdraw_from_market(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    market: String,
    amount: Uint128,
) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
    if !is_authorized(&deps.as_ref(), &info.sender)? {
        // Check authorization
        return Err(StdError::generic_err("Unauthorized"));
    }

    let current_allocation = state::MARKET_ALLOCATIONS.load(deps.storage, &market)?; // Get current allocation
    if current_allocation < amount {
        return Err(StdError::generic_err("Insufficient market allocation"));
    }

    // Create a message to withdraw funds from the market
    let withdraw_msg = WasmMsg::Execute {
        contract_addr: market.clone(),
        msg: to_json_binary(&MarketExecuteMsg::Withdraw { amount })?,
        funds: vec![],
    };

    // Update the market allocation
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

/// Pauses the contract in case of an emergency
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `info`: Message information
///
/// # Returns
/// - `StdResult<Response>`: Response with pause attribute
fn execute_emergency_pause(deps: DepsMut, info: MessageInfo) -> StdResult<Response> {
    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    if info.sender != config.governance {
        // Only governance can pause
        return Err(StdError::generic_err("Unauthorized"));
    }
    state::PAUSED.save(deps.storage, &true)?; // Set paused state
    Ok(Response::new().add_attribute("action", "emergency_pause"))
}

/// Resumes contract operations
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `info`: Message information
///
/// # Returns
/// - `StdResult<Response>`: Response with resume attribute
fn execute_resume_operations(deps: DepsMut, info: MessageInfo) -> StdResult<Response> {
    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    if info.sender != config.governance {
        // Only governance can resume
        return Err(StdError::generic_err("Unauthorized"));
    }
    state::PAUSED.save(deps.storage, &false)?; // Set active state
    Ok(Response::new().add_attribute("action", "resume_operations"))
}

/// Updates the allocation percentages to markets
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `info`: Message information
/// - `new_allocations`: New list of allocation percentages in bps
///
/// # Returns
/// - `StdResult<Response>`: Response with update attribute
fn execute_update_allocations(
    deps: DepsMut,
    info: MessageInfo,
    new_allocations: Vec<u16>,
) -> StdResult<Response> {
    let mut config = state::CONFIG.load(deps.storage)?; // Load configuration
    if info.sender != config.governance {
        // Only governance can update
        return Err(StdError::generic_err("Unauthorized"));
    }

    // Ensure the sum does not exceed 100%
    let total_bps: u16 = new_allocations.iter().sum();
    if total_bps > 10_000 {
        return Err(StdError::generic_err("Yield allocation exceeds 100%"));
    }

    // Update and save the new configuration
    config.yield_allocation_bps = new_allocations;
    state::CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "update_allocations"))
}
