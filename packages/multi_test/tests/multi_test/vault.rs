mod helpers;
use cosmwasm_std::{Addr, Attribute, Coin, Uint128};
use cw_multi_test::Executor;
use helpers::{init_user_balance, setup_vault_contract, GOVERNANCE, USDC, USER, USER1};

use perpswap::contracts::vault::{Config, ExecuteMsg, QueryMsg};
use vault::types::{PendingWithdrawalResponse, TotalAssetsResponse, VaultBalanceResponse};

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

    app.execute_contract(
        Addr::unchecked("anyone"),
        vault_addr.clone(),
        &ExecuteMsg::ProcessWithdrawal {},
        &[],
    )
    .unwrap();
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
fn test_get_total_assets() {
    let initial_balance = Coin::new(1000_u128, USDC);
    let (app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let response: TotalAssetsResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();
    assert_eq!(response.total_assets, Uint128::new(1000));
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
    assert_eq!(config.markets_allocation_bps, vec![5000, 5000]);
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
fn test_request_withdrawal_success() {
    let initial_balance = Coin::new(10000_u128, USDC);
    let (mut app, vault_addr, _) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance.clone())).unwrap();

    init_user_balance(&mut app, USER, 3000).unwrap();

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(500_u128, USDC)],
    )
    .unwrap();

    let withdrawal_amount = Uint128::new(250);

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: withdrawal_amount,
        },
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
    assert_eq!(pending.amount, withdrawal_amount);
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
fn test_emergency_pause_success() {
    let (mut app, vault_addr, _) = setup_vault_contract(vec![5000, 5000], None).unwrap();

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

    init_user_balance(&mut app, USER, 3420).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(250_u128, USDC)],
    );
    assert!(result.is_err());
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
    let (mut app, vault_addr, _) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::UpdateAllocations {
            new_allocations: vec![6000, 4000],
        },
        &[],
    )
    .unwrap();

    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    assert_eq!(config.markets_allocation_bps, vec![6000, 4000]);
}

#[test]
fn test_update_allocations_invalid_count() {
    let (mut app, vault_addr, _) = setup_vault_contract(vec![5000, 5000], None).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr,
        &ExecuteMsg::UpdateAllocations {
            new_allocations: vec![3000],
        },
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
    assert_eq!(config.markets_allocation_bps, vec![5000, 3000, 2000]);
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
fn test_redistribute_funds_no_excess() {
    let (mut app, vault_addr, _) = setup_vault_contract(vec![], None).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    );
    assert!(result.is_err());
}

#[test]
fn test_instantiate_invalid_bps() {
    let result = setup_vault_contract(vec![6000, 5000], None);
    assert!(result.is_err());
}
