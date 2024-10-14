use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use perpswap::contracts::copy_trading::ConfigUpdate;

use crate::copy_trading::load_markets;

#[test]
fn leader_config_update() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);
    let initial_config = market.query_copy_trading_config().unwrap();

    let config = ConfigUpdate {
        name: Some("new name".to_owned()),
        description: Some("new description".to_owned()),
        commission_rate: Some("0.2".parse().unwrap()),
    };
    assert_ne!(initial_config.name, config.name.clone().unwrap());
    assert_ne!(
        initial_config.description,
        config.description.clone().unwrap()
    );
    assert_ne!(
        initial_config.commission_rate,
        config.commission_rate.clone().unwrap()
    );
    // Trader cannot update leader config
    market
        .exec_copytrading(
            &trader,
            &perpswap::contracts::copy_trading::ExecuteMsg::LeaderUpdateConfig(config.clone()),
        )
        .unwrap_err();

    market
        .exec_copytrading_leader(
            &perpswap::contracts::copy_trading::ExecuteMsg::LeaderUpdateConfig(config.clone()),
        )
        .unwrap();

    let final_config = market.query_copy_trading_config().unwrap();
    assert_eq!(final_config.name, config.name.unwrap());
    assert_eq!(final_config.description, config.description.unwrap());
    assert_eq!(
        final_config.commission_rate,
        config.commission_rate.unwrap()
    );
}
