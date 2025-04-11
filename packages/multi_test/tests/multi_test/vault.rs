mod helpers;
use cosmwasm_std::{Addr, Coin, Uint128};
use cw_multi_test::Executor;
use helpers::{setup_standard_vault, setup_vault_contract, GOVERNANCE, USDC, USER, USER1};

use perpswap::{
    contracts::{
        market::entry::StatusResp,
        vault::{Config, ExecuteMsg, QueryMsg},
    },
    storage::MarketQueryMsg,
};
use vault::types::{MarketAllocationsResponse, TotalAssetsResponse, VaultBalanceResponse};

#[test]
fn test_full_flow() {                                           // Overflow: Cannot Sub with given operands 28
    let initial_balance = Coin::new(3000 as u128, USDC);
    let (mut app, vault_addr, market_addr) =
        setup_standard_vault(Some(initial_balance.clone())).unwrap();

    let user = Addr::unchecked(USER);
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(1000 as u128, USDC)],
    )
    .unwrap();

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

    let user_balance = app.wrap().query_balance(&user, USDC).unwrap().amount;
    assert_eq!(user_balance, withdrawal_amount);

    let response: MarketAllocationsResponse = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetMarketAllocations { start_after: None },
        )
        .unwrap();
    assert_eq!(response.allocations.len(), 1);
    assert_eq!(response.allocations[0].market_id, market_addr.to_string());
    assert_eq!(response.allocations[0].amount, Uint128::new(3500));
}

#[test]
fn test_get_vault_balance() {
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (app, vault_addr) =
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
fn test_request_withdrawal_zero_amount() {                                  // Overflow: Cannot Sub with given operands 113
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr) = setup_vault_contract(
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();

    let user = Addr::unchecked(USER);
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[initial_balance],
    )
    .unwrap();

    let result = app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: Uint128::zero(),
        },
        &[],
    );
    assert!(result.is_err());
}

#[test]
fn test_get_pending_withdrawal() {                                      // Overflow: Cannot Sub with given operands 142
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr) = setup_vault_contract(
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();

    let user = Addr::unchecked(USER);
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[initial_balance],
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
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let response: TotalAssetsResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();
    assert_eq!(response.total_assets, Uint128::new(1000));
}

#[test]
fn test_get_market_allocations() {                                          // Error on assert 202
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr, market_addr) =
        setup_standard_vault(Some(initial_balance.clone())).unwrap();

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

    assert_eq!(response.allocations.len(), 1);
    assert_eq!(response.allocations[0].market_id, market_addr.to_string());
    assert_eq!(response.allocations[0].amount, Uint128::new(1000));
}

#[test]
fn test_get_config() {
    let (app, vault_addr) =
        setup_vault_contract( vec![5000, 5000], None).unwrap();

    let config: perpswap::contracts::vault::Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    assert_eq!(config.governance, Addr::unchecked(GOVERNANCE));
    assert_eq!(config.usdc_denom, USDC);
    assert_eq!(config.markets_allocation_bps, vec![5000, 5000]);
    assert_eq!(config.paused, false);
}

#[test]
fn test_deposit_success() {                                     // Overflow: Cannot Sub with given operands on 236
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let deposit_amount = Coin::new(500 as u128, USDC);
    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[deposit_amount.clone()],
    )
    .unwrap();

    let lp_balance: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &vault_addr,
            &QueryMsg::GetPendingWithdrawal {
                user: USER.to_owned(),
            },
        )
        .unwrap();

    assert_eq!(lp_balance, Uint128::zero());

    let total_assets: Uint128 = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();

    assert_eq!(total_assets, Uint128::new(500));
}

#[test]
fn test_deposit_invalid_denom() {                               
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(USER),
        vault_addr,
        &ExecuteMsg::Deposit {},
        &[Coin::new(500 as u128, "uluna")],
    );
    assert!(result.is_err());
}

#[test]
fn test_request_withdrawal_success() {                                      // Overflow: Cannot Sub with given operands on 289
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr) = setup_vault_contract(
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();

    let user = Addr::unchecked(USER);
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[initial_balance.clone()],
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
fn test_process_withdrawal_success() {                                      // Overflow: Cannot Sub with given operands on 330
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr) = setup_vault_contract(
        vec![5000, 5000],
        Some(initial_balance.clone()),
    )
    .unwrap();

    let user = Addr::unchecked(USER);
    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[initial_balance.clone()],
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

    let user_balance = app.wrap().query_balance(&user, USDC).unwrap().amount;
    assert_eq!(user_balance, withdrawal_amount);

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
fn test_process_multiple_withdrawals() {                                // Overflow: Cannot Sub with given operands on 381
    let initial_balance = Coin::new(2000 as u128, USDC);
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], Some(initial_balance)).unwrap();

    let user = Addr::unchecked(USER);
    let user1 = Addr::unchecked(USER1);

    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(1000 as u128, USDC)],
    )
    .unwrap();
    app.execute_contract(
        user1.clone(),
        vault_addr.clone(),
        &ExecuteMsg::Deposit {},
        &[Coin::new(1000 as u128, USDC)],
    )
    .unwrap();

    app.execute_contract(
        user.clone(),
        vault_addr.clone(),
        &ExecuteMsg::RequestWithdrawal {
            amount: Uint128::new(500),
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
    assert_eq!(user1_balance, Uint128::new(500));
    assert_eq!(user2_balance, Uint128::new(500));
}

#[test]
fn test_redistribute_funds_success() {                              // Error getting market_lp
    let initial_balance = Coin::new(2000 as u128, USDC);
    let (mut app, vault_addr, market_addr) =
        setup_standard_vault(Some(initial_balance.clone())).unwrap();

    app.update_block(|block| block.height += 1);

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

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
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr) =
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
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], None).unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::CollectYield {},
        &[],
    )
    .unwrap();
}

#[test]
fn test_withdraw_from_market_success() {                            // Error does not find the market
    let initial_balance = Coin::new(10000 as u128, USDC);
    let (mut app, vault_addr, market_addr) =
        setup_standard_vault(Some(initial_balance)).unwrap();

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
            market: market_addr.to_string(),
            amount: Uint128::new(1000),
        },
        &[],
    )
    .unwrap();

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
fn test_emergency_pause_success() {                         // Overflow error, should be contract paused error
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], None).unwrap();

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
        &[Coin::new(500 as u128, USDC)],
    );
    assert!(result.is_err());
}

#[test]
fn test_resume_operations_success() {                      // Overflow error after resume operations
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], None).unwrap();

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

    app.execute_contract(
        Addr::unchecked(USER),
        vault_addr,
        &ExecuteMsg::Deposit {},
        &[Coin::new(500 as u128, USDC)],
    )
    .unwrap();
}

#[test]
fn test_update_allocations_success() {                      // Error Numb Allocations != Numb of Markets
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], None).unwrap();

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
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], None).unwrap();

    let result = app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr,
        &ExecuteMsg::UpdateAllocations {
            new_allocations: vec![5000],
        },
        &[],
    );
    assert!(result.is_err());
}

#[test]
fn test_instantiate_success() {
    let (app, vault_addr) =
        setup_vault_contract(vec![5000, 3000, 2000], None).unwrap();

    let config: Config = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetConfig {})
        .unwrap();

    assert_eq!(config.governance, Addr::unchecked(GOVERNANCE));
    assert_eq!(config.usdc_denom, USDC);
    assert_eq!(config.markets_allocation_bps, vec![5000, 3000, 2000]);
    assert_eq!(config.paused, false);

    let total_assets: TotalAssetsResponse = app
        .wrap()
        .query_wasm_smart(&vault_addr, &QueryMsg::GetTotalAssets {})
        .unwrap();
    assert_eq!(total_assets.total_assets, Uint128::zero());
}

#[test]
fn test_withdraw_from_market_insufficient_allocation() {
    let initial_balance = Coin::new(1000 as u128, USDC);
    let (mut app, vault_addr, market_addr) =
        setup_standard_vault(Some(initial_balance.clone())).unwrap();

    app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::RedistributeFunds {},
        &[],
    )
    .unwrap();

    let result = app.execute_contract(
        Addr::unchecked(GOVERNANCE),
        vault_addr.clone(),
        &ExecuteMsg::WithdrawFromMarket {
            market: market_addr.to_string(),
            amount: Uint128::new(1500),
        },
        &[],
    );
    assert!(result.is_err());
}

#[test]
fn test_redistribute_funds_no_excess() {
    let (mut app, vault_addr) =
        setup_vault_contract(vec![5000, 5000], None).unwrap();

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
