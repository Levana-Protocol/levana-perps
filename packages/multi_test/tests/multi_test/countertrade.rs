use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::contracts::countertrade::MarketBalance;

#[test]
fn query_config() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market.query_countertrade_config().unwrap();
}

#[test]
fn deposit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let lp = market.clone_lp(0).unwrap();

    assert_eq!(market.query_countertrade_balances(&lp).unwrap(), vec![]);

    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();
    let mut balances = market.query_countertrade_balances(&lp).unwrap();
    assert_eq!(balances.len(), 1);
    let MarketBalance {
        market: _,
        token: _,
        shares,
        collateral,
        pool_size,
    } = balances.pop().unwrap();
    assert_eq!(shares.to_string(), "100");
    assert_eq!(collateral.to_string(), "100");
    assert_eq!(pool_size.to_string(), "100");

    let lp = market.clone_lp(1).unwrap();

    market
        .exec_countertrade_mint_and_deposit(&lp, "50")
        .unwrap();
    let mut balances = market.query_countertrade_balances(&lp).unwrap();
    assert_eq!(balances.len(), 1);
    let MarketBalance {
        market: _,
        token: _,
        shares,
        collateral,
        pool_size,
    } = balances.pop().unwrap();
    assert_eq!(shares.to_string(), "50");
    assert_eq!(collateral.to_string(), "50");
    assert_eq!(pool_size.to_string(), "150");
}

#[test]
fn withdraw_no_positions() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();

    market
        .exec_countertrade_mint_and_deposit(&lp0, "100")
        .unwrap();
    market
        .exec_countertrade_mint_and_deposit(&lp1, "100")
        .unwrap();

    let balance_before = market.query_collateral_balance(&lp0).unwrap();
    market.exec_countertrade_withdraw(&lp0, "50").unwrap();
    market.exec_countertrade_withdraw(&lp0, "51").unwrap_err();
    let balance_after = market.query_collateral_balance(&lp0).unwrap();
    let expected = balance_before.checked_add("50".parse().unwrap()).unwrap();
    assert_eq!(
        expected, balance_after,
        "Before: {balance_before}. After: {balance_after}. Expected after: {expected}"
    );

    let mut balances = market.query_countertrade_balances(&lp0).unwrap();
    assert_eq!(balances.len(), 1);
    let MarketBalance {
        market: _,
        token: _,
        shares,
        collateral,
        pool_size,
    } = balances.pop().unwrap();
    assert_eq!(shares.to_string(), "50");
    assert_eq!(collateral.to_string(), "50");
    assert_eq!(pool_size.to_string(), "150");

    let mut balances = market.query_countertrade_balances(&lp1).unwrap();
    assert_eq!(balances.len(), 1);
    let MarketBalance {
        market: _,
        token: _,
        shares,
        collateral,
        pool_size,
    } = balances.pop().unwrap();
    assert_eq!(shares.to_string(), "100");
    assert_eq!(collateral.to_string(), "100");
    assert_eq!(pool_size.to_string(), "150");
}

#[test]
fn change_admin() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();

    market.exec_countertrade_accept_admin(&lp0).unwrap_err();
    market.exec_countertrade_appoint_admin(&lp0).unwrap();
    market.exec_countertrade_accept_admin(&lp1).unwrap_err();
    market.exec_countertrade_appoint_admin(&lp1).unwrap();
    market.exec_countertrade_accept_admin(&lp0).unwrap_err();
    market.exec_countertrade_accept_admin(&lp1).unwrap();
    market.exec_countertrade_appoint_admin(&lp0).unwrap_err();

    let config = market.query_countertrade_config().unwrap();
    assert_eq!(config.admin, lp1);
    assert_eq!(config.pending_admin, None);
}
