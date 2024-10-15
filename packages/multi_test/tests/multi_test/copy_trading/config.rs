use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::copy_trading::{ConfigUpdate, FactoryConfigUpdate},
    storage::FactoryExecuteMsg,
};

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
        config.commission_rate.unwrap()
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

#[test]
fn factory_config_update() {
    let perps = PerpsApp::new_cell().unwrap();
    let market = PerpsMarket::new(perps.clone()).unwrap();
    let factory = perps.borrow().factory_addr.clone();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);
    let initial_config = market.query_copy_trading_config().unwrap();

    let config = FactoryConfigUpdate {
        allowed_rebalance_queries: Some(3),
        allowed_lp_token_queries: Some(4),
    };
    assert_ne!(
        initial_config.allowed_lp_token_queries,
        config.allowed_lp_token_queries.unwrap()
    );
    assert_ne!(
        initial_config.allowed_rebalance_queries,
        config.allowed_rebalance_queries.unwrap()
    );

    // Trader cannot update factory config
    market
        .exec_copytrading(
            &trader,
            &perpswap::contracts::copy_trading::ExecuteMsg::FactoryUpdateConfig(config.clone()),
        )
        .unwrap_err();

    market
        .exec_copytrading(
            &factory,
            &perpswap::contracts::copy_trading::ExecuteMsg::FactoryUpdateConfig(config.clone()),
        )
        .unwrap();

    let final_config = market.query_copy_trading_config().unwrap();
    assert_eq!(
        final_config.allowed_lp_token_queries,
        config.allowed_lp_token_queries.unwrap()
    );
    assert_eq!(
        final_config.allowed_rebalance_queries,
        config.allowed_rebalance_queries.unwrap()
    );
}
