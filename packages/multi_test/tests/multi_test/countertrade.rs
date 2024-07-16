use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};

#[test]
fn sanity() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();

    assert_eq!(
        market.query_referees(&referrer).unwrap(),
        Vec::<Addr>::new()
    );

    assert_eq!(market.query_referrer(&referee).unwrap(), None);

    todo!()
}
