mod helpers;
use cosmwasm_std::{Addr, Coin, Uint128};
use cw_multi_test::Executor;
use helpers::{setup_market_contract, setup_vault_contract};

use perpswap::{
    contracts::{
        market::entry::StatusResp,
        vault::{Config, ExecuteMsg, QueryMsg},
    },
    storage::{MarketExecuteMsg, MarketQueryMsg},
};
use vault::types::{MarketAllocationsResponse, TotalAssetsResponse, VaultBalanceResponse};

//-------------------------------------------------------------------------------------
//                                     QUERY TESTS
//-------------------------------------------------------------------------------------

#[test]
fn test_get_vault_balance() {
    let initial_balance = Coin::new(1000 as u128, "uusd");
    let (app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance),
    )
    .unwrap();

    // Query vault balance
    let response: VaultBalanceResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetVaultBalance {})
        .unwrap();
    assert_eq!(response.vault_balance, Uint128::new(1000));
    assert_eq!(response.allocated_amount, Uint128::zero());
    assert_eq!(response.pending_withdrawals, Uint128::zero());
    assert_eq!(response.total_allocated, Uint128::new(1000));
}

#[test]
fn test_get_pending_withdrawal() {
    let initial_balance = Coin::new(1000 as u128, "uusd");
    let (mut app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();

    // Deposit
    let user = Addr::unchecked("user");
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[initial_balance],
    )
    .unwrap();

    // Request withdrawal
    let withdrawal_amount = Uint128::new(500);
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: withdrawal_amount,
        },
        &[],
    )
    .unwrap();

    // Query pending withdrawal
    let response: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(response, withdrawal_amount);
}

#[test]
fn test_get_total_assets() {
    let initial_balance = Coin::new(1000 as u128, "uusd");
    let (app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance),
    )
    .unwrap();

    // Query total assets
    let response: TotalAssetsResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();
    assert_eq!(response.total_assets, Uint128::new(1000));
}

#[test]
fn test_get_market_allocations() {
    let (mut app, vault_addr) =
        setup_vault_contract("governance", "uusd", vec![5000, 5000], None).unwrap();
    let market_addr = setup_market_contract(&mut app).unwrap();

    // Simulate allocation
    app.execute_contract(
        Addr::unchecked("governance"),
        market_addr,
        &MarketExecuteMsg::DepositLiquidity {
            stake_to_xlp: false,
        },
        &[Coin::new(500 as u128, "uusd")],
    )
    .unwrap();

    // Query allocations
    let response: MarketAllocationsResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetMarketAllocations { start_after: None },
        )
        .unwrap();
    assert_eq!(response.allocations.len(), 0);
}

#[test]
fn test_get_config() {
    let (app, vault_addr) =
        setup_vault_contract("governance", "uusd", vec![5000, 5000], None).unwrap();

    // Query config
    let config: perpswap::contracts::vault::Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();
    assert_eq!(config.governance, Addr::unchecked("governance"));
    assert_eq!(config.usdc_denom, "uusd");
    assert_eq!(config.markets_allocation_bps, vec![5000, 5000]);
    assert_eq!(config.paused, false);
}

//-------------------------------------------------------------------------------------
//                                   EXECUTE TESTS
//-------------------------------------------------------------------------------------

#[test]
fn test_deposit_success() {
    let initial_balance = Coin::new(1000 as u128, "uusd");
    let result = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance.clone()),
    );
    let (mut app, vault_addr) = result.map_err(|e| panic!("Setup failed: {:?}", e)).unwrap();

    // Execute deposit
    let user = Addr::unchecked("user");
    let deposit_amount = Coin::new(500 as u128, "uusd");
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[deposit_amount.clone()],
    )
    .unwrap();

    // Verify LP balance
    let lp_balance: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(lp_balance, Uint128::zero());

    // Verify total assets
    let total_assets: Uint128 = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();
    assert_eq!(total_assets, Uint128::new(500));
}

#[test]
fn test_deposit_invalid_denom() {
    let initial_balance = Coin::new(1000 as u128, "uusd");
    let (mut app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance),
    )
    .unwrap();

    // Try depositing wrong denom
    let result = app.execute_contract(
        Addr::unchecked("user"),
        vault_addr,
        &ExecuteMsg::Deposit {},
        &[Coin::new(500 as u128, "uluna")],
    );
    assert!(result.is_err());
}

#[test]
fn test_request_withdrawal_success() {
    let initial_balance = Coin::new(1000 as u128, "uusd");
    let (mut app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();

    // Deposit first
    let user = Addr::unchecked("user");
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[initial_balance.clone()],
    )
    .unwrap();

    // Request withdrawal
    let withdrawal_amount = Uint128::new(500);
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: withdrawal_amount,
        },
        &[],
    )
    .unwrap();

    // Verify pending withdrawal
    let pending: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(pending, withdrawal_amount);
}

#[test]
fn test_process_withdrawal_success() {
    let initial_balance = Coin::new(1000 as u128, "uusd");
    let (mut app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();

    // Deposit
    let user = Addr::unchecked("user");
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[initial_balance.clone()],
    )
    .unwrap();

    // Request withdrawal
    let withdrawal_amount = Uint128::new(500);
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: withdrawal_amount,
        },
        &[],
    )
    .unwrap();

    // Process withdrawal
    app.execute_contract(
        Addr::unchecked("anyone"),
        vault_addr.clone(),
        &ExecuteMsg::ProcessWithdrawal {},
        &[],
    )
    .unwrap();

    // Verify user received funds
    let user_balance = app.wrap().query_balance(&user, "usdc").unwrap().amount;
    assert_eq!(user_balance, withdrawal_amount);

    // Verify pending withdrawal is cleared
    let pending: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(pending, Uint128::zero());
}

#[test]
fn test_redistribute_funds_success() {
    let initial_balance = Coin::new(2000 as u128, "usdc");
    let (mut app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();

    // Set up a mock market
    let market_addr = setup_market_contract(&mut app).unwrap();

    // Register market allocation
    app.update_block(|block| block.height += 1);
    app.execute_contract(
        Addr::unchecked("governance"),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

    // Verify market received funds
    let market_lp = app
        .wrap()
        .query_wasm_smart(&market_addr, &MarketQueryMsg::Status { price: None })
        .map(|resp: StatusResp| resp.liquidity.total_lp)
        .unwrap();

    let market_lp = Uint128::from(market_lp.into_u128().unwrap());

    assert!(market_lp > Uint128::zero());
}

#[test]
fn test_redistribute_funds_unauthorized() {
    let initial_balance = Coin::new(1000 as u128, "usdc");
    let (mut app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance),
    )
    .unwrap();

    // Try redistributing as non-governance
    let result = app.execute_contract(
        Addr::unchecked("not_governance"),
        vault_addr,
        &ExecuteMsg::RedistributeFunds {},
        &[],
    );
    assert!(result.is_err());
}

#[test]
fn test_collect_yield_success() {
    let (mut app, vault_addr) =
        setup_vault_contract("governance", "uusd", vec![5000, 5000], None).unwrap();
    let _market_addr = setup_market_contract(&mut app).unwrap();

    // Register market allocation
    app.execute_contract(
        Addr::unchecked("governance"),
        vault_addr.clone(),
        &ExecuteMsg::CollectYield {},
        &[],
    )
    .unwrap();
}

#[test]
fn test_withdraw_from_market_success() {
    let initial_balance = Coin::new(1000 as u128, "usdc");
    let (mut app, vault_addr) = setup_vault_contract(
        "governance",
        "uusd",
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();
    let market_addr = setup_market_contract(&mut app).unwrap();

    // Simulate market allocation
    app.execute_contract(
        Addr::unchecked("governance"),
        market_addr.clone(),
        &MarketExecuteMsg::DepositLiquidity {
            stake_to_xlp: false,
        },
        &[Coin::new(500 as u128, "uusd")],
    )
    .unwrap();

    // Record allocation
    app.execute_contract(
        Addr::unchecked("governance"),
        vault_addr.clone(),
        &ExecuteMsg::WithdrawFromMarket {
            market: market_addr.to_string(),
            amount: Uint128::new(500),
        },
        &[],
    )
    .unwrap();

    // Verify market LP reduced
    let market_lp = app
        .wrap()
        .query_wasm_smart(
            &market_addr,
            &perpswap::storage::MarketQueryMsg::Status { price: None },
        )
        .map(|resp: perpswap::contracts::market::entry::StatusResp| resp.liquidity.total_lp)
        .unwrap();

    let market_lp = Uint128::from(market_lp.into_u128().unwrap());

    assert_eq!(market_lp, Uint128::zero());
}

#[test]
fn test_emergency_pause_success() {
    let (mut app, vault_addr) =
        setup_vault_contract("governance", "uusd", vec![5000, 5000], None).unwrap();

    app.execute_contract(
        Addr::unchecked("governance"),
        vault_addr.clone(),
        &ExecuteMsg::EmergencyPause {},
        &[],
    )
    .unwrap();

    // Verify paused
    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();
    assert!(config.paused);

    // Try depositing while paused
    let result = app.execute_contract(
        Addr::unchecked("user"),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(500 as u128, "uusd")],
    );
    assert!(result.is_err());
}

#[test]
fn test_resume_operations_success() {
    let (mut app, vault_addr) =
        setup_vault_contract("governance", "uusd", vec![5000, 5000], None).unwrap();

    // Pause first
    app.execute_contract(
        Addr::unchecked("governance"),
        vault_addr.clone(),
        &ExecuteMsg::EmergencyPause {},
        &[],
    )
    .unwrap();

    // Resume
    app.execute_contract(
        Addr::unchecked("governance"),
        vault_addr.clone(),
        &ExecuteMsg::ResumeOperations {},
        &[],
    )
    .unwrap();

    // Verify not paused
    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();
    assert!(!config.paused);

    // Deposit should now work
    app.execute_contract(
        Addr::unchecked("user"),
        vault_addr,
        &ExecuteMsg::Deposit {},
        &[Coin::new(500 as u128, "usdc")],
    )
    .unwrap();
}

#[test]
fn test_update_allocations_success() {
    let (mut app, vault_addr) =
        setup_vault_contract("governance", "usdc", vec![5000, 5000], None).unwrap();
    setup_market_contract(&mut app).unwrap();

    // Update allocations
    app.execute_contract(
        Addr::unchecked("governance"),
        vault_addr.clone(),
        &ExecuteMsg::UpdateAllocations {
            new_allocations: vec![6000, 4000],
        },
        &[],
    )
    .unwrap();

    // Verify new allocations
    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();
    assert_eq!(config.markets_allocation_bps, vec![6000, 4000]);
}

#[test]
fn test_update_allocations_invalid_count() {
    let (mut app, vault_addr) =
        setup_vault_contract("governance", "usdc", vec![5000, 5000], None).unwrap();
    setup_market_contract(&mut app).unwrap();

    // Try updating with wrong number of allocations
    let result = app.execute_contract(
        Addr::unchecked("governance"),
        vault_addr,
        &ExecuteMsg::UpdateAllocations {
            new_allocations: vec![5000],
        },
        &[],
    );
    assert!(result.is_err());
}

//-------------------------------------------------------------------------------------
//                                  INSTANTIATE TESTS
//-------------------------------------------------------------------------------------

#[test]
fn test_instantiate_success() {
    let (app, contract_addr) =
        setup_vault_contract("governance", "uusd", vec![5000, 3000, 2000], None).unwrap();

    // Query config to verify instantiation
    let config: Config = app
        .wrap()
        .query_wasm_smart(&contract_addr, &QueryMsg::GetConfig {})
        .unwrap();
    assert_eq!(config.governance, Addr::unchecked("governance"));
    assert_eq!(config.usdc_denom, "uusd");
    assert_eq!(config.markets_allocation_bps, vec![5000, 3000, 2000]);
    assert_eq!(config.paused, false);

    // Verify initial state
    let total_lp: Uint128 = app
        .wrap()
        .query_wasm_smart(&contract_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();
    assert_eq!(total_lp, Uint128::zero());
}

#[test]
fn test_instantiate_invalid_bps() {
    let result = setup_vault_contract("governance", "uusd", vec![6000, 5000], None);
    assert!(result.is_err());
}
