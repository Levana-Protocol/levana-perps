use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};

#[test]
fn query_config() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market.query_countertrade_config().unwrap();
}

#[test]
fn deposit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.exec_countertrade_mint_and_deposit(&lp, "100").unwrap();
}
