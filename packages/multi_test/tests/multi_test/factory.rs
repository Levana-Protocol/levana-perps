use cosmwasm_std::{Addr, Binary};
use levana_perpswap_multi_test::{
    config::TEST_CONFIG, market_wrapper::PerpsMarket, time::TimeJump, PerpsApp,
};
use perpswap::{
    contracts::{
        factory::entry::{CopyTradingInfoRaw, CopyTradingResp},
        market::entry::{InitialPrice, NewCopyTradingParams, NewMarketParams},
    },
    prelude::FactoryExecuteMsg,
    {namespace::FACTORY_MARKET_LAST_ADDED, storage::MarketId, time::Timestamp},
};

#[test]
fn test_factory_add_market() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let now = market.now();
    let key = FACTORY_MARKET_LAST_ADDED.as_bytes().to_vec();
    let result = market
        .query_factory_raw(Binary::new(key.clone()))
        .unwrap()
        .unwrap();
    let old_time: Timestamp = cosmwasm_std::from_json(result.as_slice()).unwrap();
    assert!(now > old_time);

    market.set_time(TimeJump::Hours(10)).unwrap();
    market
        .exec_factory(&FactoryExecuteMsg::AddMarket {
            new_market: NewMarketParams {
                market_id: MarketId::new(
                    "BTC",
                    "USD",
                    perpswap::storage::MarketType::CollateralIsQuote,
                ),
                token: market.token.clone().into(),
                config: None,
                spot_price: msg::contracts::market::spot_price::SpotPriceConfigInit::Manual {
                    admin: Addr::unchecked(TEST_CONFIG.protocol_owner.clone()).into(),
                },
                initial_borrow_fee_rate: "0.01".parse().unwrap(),
                initial_price: Some(InitialPrice {
                    price: "1".parse().unwrap(),
                    price_usd: "1".parse().unwrap(),
                }),
            },
        })
        .unwrap();

    let result = market.query_factory_raw(Binary::new(key)).unwrap().unwrap();
    let new_time: Timestamp = cosmwasm_std::from_json(result.as_slice()).unwrap();
    assert!(old_time < new_time)
}

#[test]
fn factory_has_copy_trading_contract() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let resp = market.query_factory_copy_contracts().unwrap();
    assert!(resp.addresses.len() == 1);
    assert_eq!(market.copy_trading_addr, resp.addresses[0].contract.0);
}

#[test]
fn non_admin_add_copy_trading_contract() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let name = "some_name".to_owned();
    let desc = "some_description".to_owned();

    market
        .exec_factory_as(
            &Addr::unchecked(TEST_CONFIG.protocol_owner.clone()),
            &FactoryExecuteMsg::AddCopyTrading {
                new_copy_trading: NewCopyTradingParams {
                    name: name.clone(),
                    description: desc.clone(),
                },
            },
        )
        .unwrap();
    let resp = market.query_factory_copy_contracts().unwrap();
    assert!(resp.addresses.len() == 2);
}

#[test]
fn test_copy_trading_pagination() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let name = "some_name".to_owned();
    let desc = "some_description".to_owned();

    // We start from one because the test framework already has one
    // copy trading contract
    let mut total = 1usize;
    for _ in 0..=20 {
        total += 1;
        market
            .exec_factory_as(
                &Addr::unchecked(TEST_CONFIG.protocol_owner.clone()),
                &FactoryExecuteMsg::AddCopyTrading {
                    new_copy_trading: NewCopyTradingParams {
                        name: name.clone(),
                        description: desc.clone(),
                    },
                },
            )
            .unwrap();
    }
    let old_resp = market.query_factory_copy_contracts().unwrap();
    // Can fetch max of 15 only
    assert!(old_resp.addresses.len() == 15);
    let start_after = old_resp.addresses.last().cloned();
    let resp: CopyTradingResp = market
        .query_factory(&msg::prelude::FactoryQueryMsg::CopyTrading {
            start_after: start_after.clone().map(|ct| CopyTradingInfoRaw {
                leader: ct.leader.0.into(),
                contract: ct.contract.0.into(),
            }),
            limit: None,
        })
        .unwrap();
    let start_after = start_after.unwrap();
    assert!(!resp.addresses.contains(&start_after));
    assert!(!old_resp
        .addresses
        .iter()
        .any(|item| resp.addresses.contains(item)));
    assert_eq!(resp.addresses.len() + old_resp.addresses.len(), total);
}

#[test]
fn test_copy_trading_leader_pagination() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let name = "some_name".to_owned();
    let desc = "some_description".to_owned();
    let trader = market.clone_trader(0).unwrap();

    let mut total = 0usize;
    for _ in 0..=20 {
        total += 1;
        market
            .exec_factory_as(
                &trader,
                &FactoryExecuteMsg::AddCopyTrading {
                    new_copy_trading: NewCopyTradingParams {
                        name: name.clone(),
                        description: desc.clone(),
                    },
                },
            )
            .unwrap();
    }
    let old_resp = market.query_factory_copy_contracts_leader(&trader).unwrap();
    // Can fetch max of 15 only
    assert!(old_resp.addresses.len() == 15);
    assert!(!old_resp
        .addresses
        .iter()
        .any(|item| item.leader.0 != trader.clone()));
    let start_after = old_resp.addresses.last().cloned();
    let resp: CopyTradingResp = market
        .query_factory(&msg::prelude::FactoryQueryMsg::CopyTradingForLeader {
            leader: trader.clone().into(),
            start_after: start_after.clone().map(|ct| ct.contract.0.into()),
            limit: None,
        })
        .unwrap();

    let start_after = start_after.unwrap();
    assert!(!resp.addresses.contains(&start_after));
    assert!(!old_resp
        .addresses
        .iter()
        .any(|item| resp.addresses.clone().contains(item)));
    assert!(!resp
        .addresses
        .iter()
        .any(|item| item.leader.0 != trader.clone()));
    assert_eq!(resp.addresses.len() + old_resp.addresses.len(), total);

    let resp = market
        .query_factory_copy_contracts_leader(&Addr::unchecked(TEST_CONFIG.protocol_owner.clone()))
        .unwrap();
    assert_eq!(resp.addresses.len(), 1);
}
