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
