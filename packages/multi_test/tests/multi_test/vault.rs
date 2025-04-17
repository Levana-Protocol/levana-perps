mod helpers;
use cosmwasm_std::{Addr, Attribute, Coin, Uint128};
use cw_multi_test::Executor;
use helpers::{init_user_balance, setup_vault_contract, StatusResp, GOVERNANCE, USDC, USER, USER1};
use perpswap::{
    contracts::vault::{Config, ExecuteMsg, QueryMsg},
    storage::MarketQueryMsg,
};
use std::collections::HashMap;
use vault::types::{
    MarketAllocationsResponse, PendingWithdrawalResponse, TotalAssetsResponse, VaultBalanceResponse,
};

#[test]
fn test_redistribute_funds_success() {
    let initial_balance = Coin::new(20000_u128, USDC);
    let (mut app, vault_addr, market_addr) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance.clone())).unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

    let market_lp1 = app
        .wrap()
        .query_wasm_smart(&market_addr[0], &MarketQueryMsg::Status { price: None })
        .map(|resp: StatusResp| resp.liquidity.total_lp)
        .unwrap();

    let market_lp2 = app
        .wrap()
        .query_wasm_smart(&market_addr[1], &MarketQueryMsg::Status { price: None })
        .map(|resp: StatusResp| resp.liquidity.total_lp)
        .unwrap();

    assert_eq!(market_lp1, Uint128::new(10000));
    assert_eq!(market_lp2, Uint128::new(10000));
}

#[test]
fn test_withdraw_from_market_success() {
    let initial_balance = Coin::new(10000_u128, USDC);
    let (mut app, vault_addr, market_addr) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::WithdrawFromMarket {
            market: market_addr[0].to_string(),
            amount: Uint128::new(1000),
        },
        &[],
    )
    .unwrap();

    let market_lp = app
        .wrap()
        .query_wasm_smart(
            &market_addr[0],
            &perpswap::storage::MarketQueryMsg::Status { price: None },
        )
        .map(|resp: StatusResp| resp.liquidity.total_lp)
        .unwrap();

    assert_eq!(market_lp, Uint128::new(4000));
}

#[test]
fn test_get_market_allocations() {
    let initial_balance = Coin::new(10000_u128, USDC);
    let (mut app, vault_addr, markets_addr) =
        setup_vault_contract(vec![3000, 5000, 2000], Some(initial_balance.clone())).unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

    let response: MarketAllocationsResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetMarketAllocations { start_after: None },
        )
        .unwrap();

    assert_eq!(response.allocations.len(), 3);
    assert!(
        response.allocations.iter().all(|alloc| {
            markets_addr
                .iter()
                .any(|addr| addr.to_string() == alloc.market_id)
        }),
        "Missing market id in markets_addr"
    );
    assert_eq!(response.allocations[0].amount, Uint128::new(5000));
}

#[test]
fn test_full_flow() {
    let (mut app, vault_addr, markets_addr) = setup_vault_contract(vec![5000], None).unwrap();

    let user = Addr::unchecked(USER);
    init_user_balance(&mut app, USER, 2500).unwrap();

    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(1000_u128, USDC)],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::CollectYield {},
        &[],
    )
    .unwrap();

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

    app.execute_contract(
        Addr::unchecked("anyone"),
        vault_addr.clone(),
        &ExecuteMsg::ProcessWithdrawal {},
        &[],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

    let user_balance = app.wrap().query_balance(&user, USDC).unwrap().amount;

    assert_eq!(user_balance, Uint128::new(2000));

    let response: MarketAllocationsResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetMarketAllocations { start_after: None },
        )
        .unwrap();

    assert_eq!(response.allocations.len(), 1);
    assert_eq!(
        response.allocations[0].market_id,
        markets_addr[0].to_string()
    );
    assert_eq!(response.allocations[0].amount, Uint128::new(500));
}

#[test]
fn test_get_vault_balance() {
    let initial_balance = Coin::new(1000_u128, USDC);
    let (app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let response: VaultBalanceResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetVaultBalance {})
        .unwrap();

    assert_eq!(response.vault_balance, Uint128::new(1000));
    assert_eq!(response.allocated_amount, Uint128::zero());
    assert_eq!(response.pending_withdrawals, Uint128::zero());
    assert_eq!(response.total_allocated, Uint128::new(1000));

    let response: TotalAssetsResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();

    assert_eq!(response.total_assets, Uint128::new(1000));
}

#[test]
fn test_request_user_withdrawal() {
    let (mut app, vault_addr, _) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    init_user_balance(&mut app, USER, 8237).unwrap();

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(500_u128, USDC)],
    )
    .unwrap();

    let result = app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: Uint128::zero(),
        },
        &[],
    );

    assert!(result.is_err());

    let result = app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: Uint128::new(250),
        },
        &[],
    );

    assert!(result.is_ok());

    let pending: PendingWithdrawalResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: USER.to_string(),
            },
        )
        .unwrap();

    assert_eq!(pending.amount, Uint128::new(250));

    app.execute_contract(
        Addr::unchecked("anyone"),
        vault_addr.clone(),
        &ExecuteMsg::ProcessWithdrawal {},
        &[],
    )
    .unwrap();

    let pending: PendingWithdrawalResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: USER.to_string(),
            },
        )
        .unwrap();

    assert_eq!(pending.amount, Uint128::zero());
}

#[test]
fn test_get_pending_withdrawal() {
    let initial_balance = Coin::new(10000_u128, USDC);
    let (mut app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance.clone())).unwrap();

    init_user_balance(&mut app, USER, 3000).unwrap();

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(1000_u128, USDC)],
    )
    .unwrap();

    let withdrawal_amount = Uint128::new(500);
    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: withdrawal_amount,
        },
        &[],
    )
    .unwrap();

    let response: PendingWithdrawalResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: USER.to_string(),
            },
        )
        .unwrap();

    assert_eq!(response.amount, withdrawal_amount);
}

#[test]
fn test_get_config() {
    let (app, vault_addr, _) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    let config: perpswap::contracts::vault::Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    assert_eq!(config.governance, Addr::unchecked(GOVERNANCE));
    assert_eq!(config.usdc_denom, USDC);
    assert_eq!(
        config
            .markets_allocation_bps
            .values()
            .copied()
            .collect::<Vec<u16>>(),
        vec![5000, 5000]
    );
    assert!(!config.paused);
}

#[test]
fn test_deposit_success() {
    let (mut app, vault_addr, _) = setup_vault_contract(vec![], None).unwrap();

    init_user_balance(&mut app, USER, 5000).unwrap();

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(500_u128, USDC)],
    )
    .unwrap();

    let vault_balance: VaultBalanceResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetVaultBalance {})
        .unwrap();

    assert_eq!(vault_balance.vault_balance, Uint128::new(500));

    let lp_balance: PendingWithdrawalResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: USER.to_owned(),
            },
        )
        .unwrap();

    assert_eq!(lp_balance.amount, Uint128::zero());

    let total_assets: TotalAssetsResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();

    assert_eq!(total_assets.total_assets, Uint128::new(500));
}

#[test]
fn test_deposit_invalid_denom() {
    let initial_balance = Coin::new(1000_u128, USDC);
    let (mut app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(USER),
        vault_addr,
        &ExecuteMsg::Deposit {},
        &[Coin::new(500_u128, "uluna")],
    );
    assert!(result.is_err());
}

#[test]
fn test_process_withdrawal_success() {
    let initial_balance = Coin::new(1000_u128, USDC);
    let (mut app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance.clone())).unwrap();

    init_user_balance(&mut app, USER, 7000).unwrap();

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(3000_u128, USDC)],
    )
    .unwrap();

    let withdrawal_amount = Uint128::new(1000);

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: withdrawal_amount,
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked("anyone"),
        vault_addr.clone(),
        &ExecuteMsg::ProcessWithdrawal {},
        &[],
    )
    .unwrap();

    let user_balance = app
        .wrap()
        .query_balance(Addr::unchecked(USER), USDC)
        .unwrap()
        .amount;

    assert_eq!(user_balance, Uint128::new(5000));

    let pending: PendingWithdrawalResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: USER.to_string(),
            },
        )
        .unwrap();
    assert_eq!(pending.amount, Uint128::zero());
}

#[test]
fn test_process_multiple_withdrawals() {
    let initial_balance = Coin::new(2000_u128, USDC);
    let (mut app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    init_user_balance(&mut app, USER, 7000).unwrap();
    init_user_balance(&mut app, USER1, 2500).unwrap();
    let user = Addr::unchecked(USER);
    let user1 = Addr::unchecked(USER1);

    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(4000_u128, USDC)],
    )
    .unwrap();

    app.execute_contract(
        user1.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(2000_u128, USDC)],
    )
    .unwrap();

    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: Uint128::new(1500),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        user1.clone(),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: Uint128::new(500),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked("anyone"),
        vault_addr.clone(),
        &ExecuteMsg::ProcessWithdrawal {},
        &[],
    )
    .unwrap();

    let user1_balance = app.wrap().query_balance(&user, USDC).unwrap().amount;
    let user2_balance = app.wrap().query_balance(&user1, USDC).unwrap().amount;

    assert_eq!(user1_balance, Uint128::new(4500));
    assert_eq!(user2_balance, Uint128::new(1000));
}

#[test]
fn test_redistribute_funds_unauthorized() {
    let initial_balance = Coin::new(1000_u128, USDC);
    let (mut app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(USER),
        vault_addr,
        &ExecuteMsg::RedistributeFunds {},
        &[],
    );
    assert!(result.is_err());
}

#[test]
fn test_collect_yield_success() {
    let (mut app, vault_addr, markets_addr) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    let response = app
        .execute_contract(
            Addr::unchecked(GOVERNANCE),
            vault_addr.clone(),
            &ExecuteMsg::CollectYield {},
            &[],
        )
        .unwrap();

    let wasm_event = response
        .events
        .iter()
        .find(|e| {
            e.ty == "wasm"
                && e.attributes
                    .iter()
                    .any(|attr| attr.key == "action" && attr.value == "collect_yield")
        })
        .expect("WASM event not found");

    println!("{:?}", wasm_event);

    assert_eq!(
        wasm_event
            .attributes
            .iter()
            .filter(|attr| attr.key != "_contract_address")
            .collect::<Vec<_>>(),
        vec![
            Attribute {
                key: "action".to_string(),
                value: "collect_yield".to_string()
            },
            Attribute {
                key: "markets_processed".to_string(),
                value: markets_addr.len().to_string()
            },
        ]
    );
}

#[test]
fn test_resume_operations_success() {
    let (mut app, vault_addr, _) = setup_vault_contract(vec![], None).unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::EmergencyPause {},
        &[],
    )
    .unwrap();

    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    assert!(config.paused);

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::ResumeOperations {},
        &[],
    )
    .unwrap();

    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    assert!(!config.paused);

    init_user_balance(&mut app, USER, 1540).unwrap();

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr,
        &ExecuteMsg::Deposit {},
        &[Coin::new(500_u128, USDC)],
    )
    .unwrap();
}

#[test]
fn test_update_allocations_success() {
    let (mut app, vault_addr, markets_addr) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    let bps = vec![6000, 4000];

    let new_allocations: HashMap<String, u16> = markets_addr
        .iter()
        .map(|a| a.to_string())
        .zip(bps)
        .collect();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::UpdateAllocations { new_allocations },
        &[],
    )
    .unwrap();

    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    let mut markets_allocation_bps: Vec<u16> =
        config.markets_allocation_bps.values().copied().collect();

    markets_allocation_bps.sort();

    assert_eq!(markets_allocation_bps, vec![4000, 6000]);
}

#[test]
fn test_update_allocations_invalid_count() {
    let (mut app, vault_addr, markets_addr) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    let bps = vec![6000];

    let new_allocations: HashMap<String, u16> = markets_addr
        .iter()
        .map(|a| a.to_string())
        .zip(bps)
        .collect();

    let result = app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr,
        &ExecuteMsg::UpdateAllocations { new_allocations },
        &[],
    );

    assert!(result.is_err());
}

#[test]
fn test_instantiate_success() {
    let (app, vault_addr, _) = setup_vault_contract(vec![5000, 3000, 2000], None).unwrap();

    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    assert_eq!(config.governance, Addr::unchecked(GOVERNANCE));
    assert_eq!(config.usdc_denom, USDC);

    let mut markets_allocation_bps: Vec<u16> =
        config.markets_allocation_bps.values().copied().collect();

    markets_allocation_bps.sort();

    assert_eq!(markets_allocation_bps, vec![2000, 3000, 5000]);
    assert!(!config.paused);

    let total_assets: TotalAssetsResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();

    assert_eq!(total_assets.total_assets, Uint128::zero());
}

#[test]
fn test_withdraw_from_market_insufficient_allocation() {
    let initial_balance = Coin::new(1000_u128, USDC);
    let (mut app, vault_addr, markets_addr) =
        setup_vault_contract(vec![1000], Some(initial_balance)).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::WithdrawFromMarket {
            market: markets_addr[0].to_string(),
            amount: Uint128::new(1500),
        },
        &[],
    );

    assert!(result.is_err());
}

#[test]
fn test_instantiate_invalid_bps() {
    let result = setup_vault_contract(vec![6000, 5000], None);
    assert!(result.is_err());
}

#[test]
fn deposit_zero_collateral_fails() {
    let (mut app, vault_addr, _) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    let user = Addr::unchecked(USER);
    init_user_balance(&mut app, USER, 1000).unwrap();

    let balance_before = app.wrap().query_balance(&user, USDC).unwrap().amount;

    let vault_balance_before: VaultBalanceResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetVaultBalance {})
        .unwrap();

    let result = app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(0_u128, USDC)],
    );

    assert!(result.is_err(), "Deposit with zero collateral should fail");

    let balance_after = app.wrap().query_balance(&user, USDC).unwrap().amount;

    let vault_balance_after: VaultBalanceResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetVaultBalance {})
        .unwrap();

    assert_eq!(
        balance_before, balance_after,
        "User balance should not change"
    );

    assert_eq!(
        vault_balance_before.vault_balance, vault_balance_after.vault_balance,
        "Vault balance should not change"
    );
}

#[test]
fn withdraw_all_from_market_success() {
    let initial_balance = Coin::new(10000_u128, USDC);
    let (mut app, vault_addr, market_addr) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

    let allocations_before: MarketAllocationsResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetMarketAllocations { start_after: None },
        )
        .unwrap();

    assert_eq!(allocations_before.allocations[0].amount, Uint128::new(5000));

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::WithdrawFromMarket {
            market: market_addr[0].to_string(),
            amount: Uint128::new(5000),
        },
        &[],
    )
    .unwrap();

    let allocations_after: MarketAllocationsResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetMarketAllocations { start_after: None },
        )
        .unwrap();

    let market_allocation = allocations_after
        .allocations
        .iter()
        .find(|alloc| alloc.market_id == market_addr[0].to_string())
        .unwrap();

    assert_eq!(
        market_allocation.amount,
        Uint128::zero(),
        "Market allocation should be zero"
    );

    let other_market_allocation = allocations_after
        .allocations
        .iter()
        .find(|alloc| alloc.market_id == market_addr[1].to_string())
        .unwrap();

    assert_eq!(
        other_market_allocation.amount,
        Uint128::new(5000),
        "Other market allocation should remain unchanged"
    );

    let vault_balance: VaultBalanceResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetVaultBalance {})
        .unwrap();

    assert_eq!(
        vault_balance.total_allocated,
        Uint128::new(5000),
        "Total allocated should reflect withdrawal"
    );
}

#[test]
fn emergency_pause_blocks_pending_operations() {
    let initial_balance = Coin::new(10000_u128, USDC);
    let (mut app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let user = Addr::unchecked(USER);
    init_user_balance(&mut app, USER, 5000).unwrap();

    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(2000_u128, USDC)],
    )
    .unwrap();

    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: Uint128::new(1000),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::EmergencyPause {},
        &[],
    )
    .unwrap();

    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    assert!(config.paused);

    let result = app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(250_u128, USDC)],
    );

    assert!(result.is_err());

    let result = app.execute_contract(
        Addr::unchecked("anyone"),
        vault_addr.clone(),
        &ExecuteMsg::ProcessWithdrawal {},
        &[],
    );

    assert!(
        result.is_err(),
        "ProcessWithdrawal should fail during pause"
    );

    let pending: PendingWithdrawalResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: USER.to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        pending.amount,
        Uint128::new(1000),
        "User has pending withdrawals"
    );

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::ResumeOperations {},
        &[],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked("anyone"),
        vault_addr.clone(),
        &ExecuteMsg::ProcessWithdrawal {},
        &[],
    )
    .unwrap();

    let user_balance = app.wrap().query_balance(&user, USDC).unwrap().amount;
    assert_eq!(
        user_balance,
        Uint128::new(4000),
        "User balance should reflect withdrawal"
    );

    let pending: PendingWithdrawalResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: USER.to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        pending.amount,
        Uint128::zero(),
        "No pending withdrawals after processing"
    );
}

#[test]
fn redistribute_funds_no_liquidity() {
    let (mut app, vault_addr, market_addr) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    );

    assert!(
        result.is_err(),
        "RedistributeFunds should fail with no liquidity"
    );

    let user = Addr::unchecked(USER);
    init_user_balance(&mut app, USER, 10000).unwrap();

    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(10000_u128, USDC)],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

    let market_lp1 = app
        .wrap()
        .query_wasm_smart(&market_addr[0], &MarketQueryMsg::Status { price: None })
        .map(|resp: StatusResp| resp.liquidity.total_lp)
        .unwrap();

    let market_lp2 = app
        .wrap()
        .query_wasm_smart(&market_addr[1], &MarketQueryMsg::Status { price: None })
        .map(|resp: StatusResp| resp.liquidity.total_lp)
        .unwrap();

    assert_eq!(
        market_lp1,
        Uint128::new(5000),
        "Market 1 should have allocated funds"
    );

    assert_eq!(
        market_lp2,
        Uint128::new(5000),
        "Market 2 should have allocated funds"
    );
}
