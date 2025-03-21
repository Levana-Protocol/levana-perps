use perpswap::{
    contracts::{market::entry::StatusResp, vault::ExecuteMsg},
    number::{LpToken, NonZero},
    storage::{MarketExecuteMsg, MarketQueryMsg},
    token::Token,
};

use crate::{
    common::{check_not_paused, get_total_assets, is_authorized},
    prelude::*,
    state::{self},
    types::WithdrawalRequest,
};

#[derive(Serialize, Deserialize)]
struct GetUtilizationResponse {
    utilization: Uint128, // Market utilization level
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
        ExecuteMsg::Deposit {} => execute_deposit(deps, env, info),

        ExecuteMsg::RequestWithdrawal { amount } => {
            execute_request_withdrawal(deps, env, info, amount)
        }

        ExecuteMsg::RedistributeFunds {} => execute_redistribute_funds(deps, env, info),

        ExecuteMsg::CollectYield { batch_limit } => execute_collect_yield(deps, info, batch_limit),

        ExecuteMsg::ProcessWithdrawal {} => execute_process_withdrawal(deps, env, info),

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
///
/// # Returns
/// - `StdResult<Response>`: Response with mint message and attributes
fn execute_deposit(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let sender = info.sender.clone(); // Address of the user depositing

    // Get the amount of USDC sent
    let usdc = info
        .funds
        .iter()
        .find(|c| c.denom == config.usdc_denom)
        .ok_or_else(|| StdError::generic_err("No USDC sent"))?;
    let amount = usdc.amount; // Use the sent amount directly

    if amount.is_zero() {
        return Err(StdError::generic_err(
            "Deposit amount must be greater than zero",
        ));
    }

    // Calculate the amount of USDCLP tokens to mint based on total assets and LP supply
    // Note: This assumes total_assets reflects the net value without impairment.
    // Future complexity (e.g., impairment, fees) may require adjusting total_assets or adding factors.
    let total_assets = get_total_assets(deps.as_ref(), &env)?;
    let total_lp = state::TOTAL_LP_SUPPLY.load(deps.storage)?;
    let lp_amount = if total_lp.is_zero() {
        amount // Initial deposit: 1:1 ratio
    } else {
        amount.multiply_ratio(total_lp, total_assets.total_assets) // Proportional to existing LP
    };

    // Create a message to mint USDCLP tokens as CW20
    let mint_msg = WasmMsg::Execute {
        contract_addr: config.usdclp_address,
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
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let sender = info.sender.clone(); // Address of the user requesting withdrawal

    // Create a message to burn the user's USDCLP tokens
    let burn_msg = WasmMsg::Execute {
        contract_addr: config.usdclp_address,
        msg: to_json_binary(&Cw20ExecuteMsg::BurnFrom {
            owner: sender.to_string(),
            amount,
        })?,
        funds: vec![],
    };

    // Add the withdrawal request to a FIFO queue
    let withdrawal_request = WithdrawalRequest {
        user: sender.to_string(),
        amount,
        timestamp: env.block.time.into(), // Use block timestamp to determine order of arrival
    };
    state::WITHDRAWAL_QUEUE.update(
        deps.storage,
        |mut queue| -> StdResult<Vec<WithdrawalRequest>> {
            queue.push(withdrawal_request); // Append to the end for FIFO
            Ok(queue)
        },
    )?;

    // Reduce the total LP supply, handling underflow
    state::TOTAL_LP_SUPPLY.update(deps.storage, |t| -> StdResult<Uint128> {
        Ok(t.checked_sub(amount).expect("Insufficient LP supply"))
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
// Execute function: Redistributes excess funds to markets
fn execute_redistribute_funds(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    // Ensure the contract is not paused
    check_not_paused(&deps.as_ref())?;
    // Verify the sender is authorized
    // Note: This is permissioned because it likely redistributes funds (e.g., processing withdrawals or reallocating market funds),
    // which requires control to prevent unauthorized access or abuse. Restricted to governance/operators for security.
    if !is_authorized(&deps.as_ref(), &info.sender)? {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // Load the vault's configuration
    let config = state::CONFIG.load(deps.storage)?;

    // Calculate the total pending withdrawals by summing values in PENDING_WITHDRAWALS
    let pending = state::TOTAL_PENDING_WITHDRAWALS
        .load(deps.storage)
        .unwrap_or(Uint128::zero());

    // Get the contract's native USDC balance
    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;

    // Calculate the available excess by subtracting pending withdrawals from the balance
    let excess = vault_balance
        .checked_sub(pending)
        .unwrap_or(Uint128::zero());

    // Fail if there is no excess to redistribute
    if excess.is_zero() {
        return Err(StdError::generic_err("No excess to redistribute"));
    }

    // Retrieve the list of markets from the keys of MARKET_ALLOCATIONS
    // No external factory is needed since markets are tracked internally
    let markets: Vec<String> = state::MARKET_ALLOCATIONS
        .keys(deps.storage, None, None, Order::Ascending)
        .filter_map(|market_id_res| {
            let market_id = market_id_res.ok()?;
            let resp: StatusResp = deps
                .querier
                .query(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: market_id.clone(),
                    msg: to_json_binary(&MarketQueryMsg::Status { price: None }).ok()?,
                }))
                .ok()?;
            if let Token::Native { denom, .. } = &resp.collateral {
                if denom == &config.usdc_denom {
                    Some(Ok(market_id))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<StdResult<Vec<String>>>()?;

    // Calculate each market's utilization and sort by highest utilization
    let mut utilizations: Vec<(String, Uint128)> = markets
        .into_iter()
        .filter_map(|market| {
            // Query the market status
            let resp: StatusResp = deps
                .querier
                .query(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: market.clone(),
                    msg: to_json_binary(&MarketQueryMsg::Status { price: None })
                        .expect("Serialize Market Query Msg"),
                }))
                .ok()?;

            // Calculate utilization as total_lp + total_xlp, defaulting to 0 on overflow
            let utilization =
                (resp.liquidity.total_lp + resp.liquidity.total_xlp).unwrap_or_default();

            // Convert to Uint128
            let value = Uint128::from(utilization.into_u128().expect("Error LpToken to Uint128"));
            Some((market, value))
        })
        .collect();

    // Sort by utilization, highest to lowest
    utilizations.sort_by(|a, b| b.1.cmp(&a.1));

    // Sort by utilization, highest to lowest
    utilizations.sort_by(|a, b| b.1.cmp(&a.1));

    // Sum the total basis points for distribution
    let total_bps: u16 = config.markets_allocation_bps.iter().sum();
    if total_bps == 0 {
        return Err(StdError::generic_err("No allocation percentages defined"));
    }

    // List of Cosmos messages to execute
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut remaining = excess;

    // Distribute excess funds across markets based on basis points
    for (i, bps) in config.markets_allocation_bps.iter().enumerate() {
        if let Some((market, _)) = utilizations.get(i) {
            let amount = excess.multiply_ratio(*bps as u128, total_bps as u128);
            if !amount.is_zero() {
                // Create a message to deposit USDC into the market
                let deposit_msg = WasmMsg::Execute {
                    contract_addr: market.clone(),
                    msg: to_json_binary(&MarketExecuteMsg::DepositLiquidity {
                        stake_to_xlp: false, // Set to true if you want to stake to xLP
                    })?,
                    funds: vec![Coin {
                        denom: config.usdc_denom.clone(),
                        amount,
                    }],
                };
                messages.push(deposit_msg.into());

                // Update MARKET_ALLOCATIONS with the new allocated amount
                state::MARKET_ALLOCATIONS.update(
                    deps.storage,
                    market.as_str(),
                    |a| -> Result<Uint128, StdError> { Ok(a.unwrap_or(Uint128::zero()) + amount) },
                )?;

                // Reduce the remaining amount
                remaining = remaining.checked_sub(amount).unwrap_or(Uint128::zero());
            }
        }
    }

    // If there's any remaining amount, send it to governance
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

    // Build and return the response with messages and attributes
    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "redistribute_funds"),
        ("excess", &excess.to_string()),
        ("remaining", &remaining.to_string()),
    ]))
}

/// Collects yields from markets into the vault
///
/// # Parameters
/// - `deps`: Mutable dependencies
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
                                       // Verify the sender is authorized
                                       // Note: Permissioned because it collects USDC yield from markets to the vault, a sensitive operation.
                                       // Restricted to governance/operators to prevent unauthorized fund movement and ensure strategic timing.
    if !is_authorized(&deps.as_ref(), &info.sender)? {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let limit = batch_limit.unwrap_or(20).min(50); // Batch limit, max 50

    // Retrieve markets from MARKET_ALLOCATIONS instead of querying the factory
    let markets: Vec<String> = state::MARKET_ALLOCATIONS
        .keys(deps.storage, None, None, Order::Ascending)
        .filter_map(|market_id_res| {
            let market_id = market_id_res.ok()?;
            // Optional: Filter for USDC markets (if StatusResp provides collateral info)
            let resp: StatusResp = deps
                .querier
                .query(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: market_id.clone(),
                    msg: to_json_binary(&MarketQueryMsg::Status { price: None }).ok()?,
                }))
                .ok()?;
            if let Token::Native { denom, .. } = &resp.collateral {
                if denom == &config.usdc_denom {
                    Some(Ok(market_id))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .take(limit as usize) // Apply batch limit
        .collect::<StdResult<Vec<String>>>()?;

    // Create messages to claim yields from markets
    let messages: Vec<CosmosMsg> = markets
        .iter()
        .filter_map(|market| {
            Some(
                WasmMsg::Execute {
                    contract_addr: market.to_string(),
                    msg: to_json_binary(&MarketExecuteMsg::ClaimYield {}).ok()?,
                    funds: vec![],
                }
                .into(),
            )
        })
        .collect();

    // Return a response with messages and attribute
    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "collect_yield")
        .add_attribute("markets_processed", markets.len().to_string()))
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
///   Processes pending withdrawals from the vault in FIFO order
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `env`: Contract environment
/// - `info`: Message information
///
/// # Returns
/// - `StdResult<Response>`: Response with withdrawal messages and attributes
fn execute_process_withdrawal(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo, // Sender not used for authorization
) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
                                       // No authorization check - open to all users to process pending withdrawals

    let config = state::CONFIG.load(deps.storage)?; // Load configuration
    let mut queue = state::WITHDRAWAL_QUEUE.load(deps.storage)?; // Load the withdrawal queue

    // Check if there are any pending withdrawals
    if queue.is_empty() {
        return Err(StdError::generic_err("No pending withdrawals to process"));
    }

    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, &config.usdc_denom)?
        .amount;
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut processed_amount = Uint128::zero();
    let limit = 20; // Fixed limit for gas efficiency (configurable if needed)

    // Process pending withdrawals in FIFO order up to limit or available funds
    for _ in 0..limit {
        if let Some(request) = queue.first() {
            if processed_amount + request.amount > vault_balance {
                break; // Stop if insufficient funds
            }

            let user_addr = deps.api.addr_validate(&request.user)?;
            messages.push(
                BankMsg::Send {
                    to_address: user_addr.to_string(),
                    amount: vec![Coin {
                        denom: config.usdc_denom.clone(),
                        amount: request.amount,
                    }],
                }
                .into(),
            );

            processed_amount += request.amount;
            queue.remove(0);
        } else {
            break;
        }
    }

    // Update total pending withdrawals
    state::TOTAL_PENDING_WITHDRAWALS.update(deps.storage, |total| -> StdResult<Uint128> {
        Ok(total
            .checked_sub(processed_amount)
            .expect("Underflow in total pending withdrawals"))
    })?;

    state::WITHDRAWAL_QUEUE.save(deps.storage, &queue)?;

    Ok(Response::new()
        .add_messages(messages.clone())
        .add_attribute("action", "process_withdrawal")
        .add_attribute("processed_count", messages.len().to_string())
        .add_attribute("processed_amount", processed_amount.to_string()))
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
// Function to withdraw funds from a market
fn execute_withdraw_from_market(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    market: String,
    amount: Uint128,
) -> StdResult<Response> {
    check_not_paused(&deps.as_ref())?; // Ensure the contract is not paused
    if !is_authorized(&deps.as_ref(), &info.sender)? {
        // Check if the sender is authorized
        return Err(StdError::generic_err("Unauthorized"));
    }

    let current_allocation = state::MARKET_ALLOCATIONS.load(deps.storage, &market)?; // Get the current allocation for the market
    if current_allocation < amount {
        return Err(StdError::generic_err("Insufficient market allocation"));
    }

    // Convert Uint128 to LpToken and then to NonZero<LpToken>
    let lp_amount = LpToken::from_u128(amount.into()).expect("Can't convert Uint128 to LpToken");
    let lp_amount = Some(
        NonZero::new(lp_amount).ok_or_else(|| StdError::generic_err("Amount must be non-zero"))?,
    );

    // Create a message to withdraw funds from the market
    let withdraw_msg = WasmMsg::Execute {
        contract_addr: market.clone(),
        msg: to_json_binary(&MarketExecuteMsg::WithdrawLiquidity {
            lp_amount,
            claim_yield: false, // Set to true if you want to claim yield as well
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

/// Pauses the contract in case of an emergency
///
/// # Parameters
/// - `deps`: Mutable dependencies
/// - `info`: Message information
///
/// # Returns
/// - `StdResult<Response>`: Response with pause attribute
fn execute_emergency_pause(deps: DepsMut, info: MessageInfo) -> StdResult<Response> {
    // Load the current configuration from storage
    let mut config = state::CONFIG.load(deps.storage)?;

    // Check if the sender is the governance address; fail if not
    if info.sender != config.governance {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // Set the paused field to true to pause the contract
    config.paused = true;

    // Save the updated configuration back to storage
    state::CONFIG.save(deps.storage, &config)?;

    // Return a response with an attribute indicating the action
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
    // Load the current configuration from storage
    let mut config = state::CONFIG.load(deps.storage)?;

    // Check if the sender is the governance address; fail if not
    if info.sender != config.governance {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // Set the paused field to true to pause the contract
    config.paused = false;

    // Save the updated configuration back to storage
    state::CONFIG.save(deps.storage, &config)?;
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

    // Count the number of markets in MARKET_ALLOCATIONS
    let market_count = state::MARKET_ALLOCATIONS
        .keys(deps.storage, None, None, Order::Ascending)
        .count();
    if new_allocations.len() != market_count {
        return Err(StdError::generic_err(format!(
            "Number of allocations ({}) must match number of markets ({})",
            new_allocations.len(),
            market_count
        )));
    }

    // Ensure the sum does not exceed 100%
    let total_bps: u16 = new_allocations.iter().sum();
    if total_bps > 10_000 {
        return Err(StdError::generic_err("Market allocation exceeds 100%"));
    }

    // Update and save the new configuration
    config.markets_allocation_bps = new_allocations;
    state::CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "update_allocations"))
}
