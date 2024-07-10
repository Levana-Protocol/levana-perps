use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};

#[test]
fn no_initial_referrer() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();

    assert_eq!(
        market.query_referees(&referrer).unwrap(),
        Vec::<Addr>::new()
    );

    assert_eq!(market.query_referrer(&referee).unwrap(), None);
}

#[test]
fn register_referrer() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();

    market.exec_register_referrer(&referee, &referrer).unwrap();
    assert_eq!(
        market.query_referees(&referrer).unwrap(),
        vec![referee.clone()]
    );

    assert_eq!(market.query_referrer(&referee).unwrap(), Some(referrer));
}

#[test]
fn cannot_register_twice() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();

    market.exec_register_referrer(&referee, &referrer).unwrap();
    market
        .exec_register_referrer(&referee, &referrer)
        .unwrap_err();
}

#[test]
fn enumeration_works() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let mut referees = (1..50)
        .map(|i| market.clone_trader(i).unwrap())
        .collect::<Vec<_>>();

    for referee in &referees {
        market.exec_register_referrer(referee, &referrer).unwrap();
    }

    for referee in &referees {
        assert_eq!(
            market.query_referrer(referee).unwrap().as_ref(),
            Some(&referrer)
        );
    }

    referees.sort();

    assert_eq!(market.query_referees(&referrer).unwrap(), referees);
}
