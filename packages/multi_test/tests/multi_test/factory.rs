use cosmwasm_std::{Addr, Binary};
use levana_perpswap_multi_test::{
    config::TEST_CONFIG, market_wrapper::PerpsMarket, time::TimeJump, PerpsApp,
};
use msg::{
    contracts::{
        factory::entry::CopyTradingResp,
        market::entry::{InitialPrice, NewCopyTradingParams, NewMarketParams},
    },
    prelude::FactoryExecuteMsg,
    shared::{namespace::FACTORY_MARKET_LAST_ADDED, storage::MarketId, time::Timestamp},
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
                    msg::shared::storage::MarketType::CollateralIsQuote,
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
    assert!(resp.copy_trading_addresses.len() == 1);
    assert_eq!(market.copy_trading_addr, resp.copy_trading_addresses[0]);
}

#[test]
fn non_admin_add_copy_trading_contract() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let name = "some_name".to_owned();
    let desc = "some_description".to_owned();

    let trader = market.clone_trader(0).unwrap();
    market
        .exec_factory_as(
            &trader,
            &FactoryExecuteMsg::AddCopyTrading {
                new_copy_trading: NewCopyTradingParams {
                    leader: trader.clone().into(),
                    name: name.clone(),
                    description: desc.clone(),
                },
            },
        )
        .unwrap_err();

    // But should be able to add new copy trading contract as protocol
    // owner
    market
        .exec_factory_as(
            &Addr::unchecked(TEST_CONFIG.protocol_owner.clone()),
            &FactoryExecuteMsg::AddCopyTrading {
                new_copy_trading: NewCopyTradingParams {
                    leader: trader.clone().into(),
                    name: name.clone(),
                    description: desc.clone(),
                },
            },
        )
        .unwrap();
    let resp = market.query_factory_copy_contracts().unwrap();
    assert!(resp.copy_trading_addresses.len() == 2);
}

#[test]
fn test_copy_trading_pagination() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let name = "some_name".to_owned();
    let desc = "some_description".to_owned();
    let trader = market.clone_trader(0).unwrap();

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
                        leader: trader.clone().into(),
                        name: name.clone(),
                        description: desc.clone(),
                    },
                },
            )
            .unwrap();
    }
    let old_resp = market.query_factory_copy_contracts().unwrap();
    // Can fetch max of 15 only
    assert!(old_resp.copy_trading_addresses.len() == 15);
    let start_after = old_resp.copy_trading_addresses.last().cloned();
    let resp: CopyTradingResp = market
        .query_factory(&msg::prelude::FactoryQueryMsg::CopyTrading {
            start_after: start_after.clone().map(|addr| addr.into()),
            limit: None,
        })
        .unwrap();
    let start_after = start_after.unwrap();
    assert!(!resp.copy_trading_addresses.contains(&start_after));
    assert!(!old_resp
        .copy_trading_addresses
        .iter()
        .any(|item| resp.copy_trading_addresses.contains(item)));
    assert_eq!(
        resp.copy_trading_addresses.len() + old_resp.copy_trading_addresses.len(),
        total
    );
}
