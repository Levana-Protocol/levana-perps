use cosmwasm_std::{Addr, Attribute, Binary, Event};
use levana_perpswap_multi_test::{
    config::TEST_CONFIG, market_wrapper::PerpsMarket, time::TimeJump, PerpsApp,
};
use perpswap::{
    contracts::{
        factory::entry::{CopyTradingInfoRaw, CopyTradingResp, CounterTradeResp},
        market::entry::{
            InitialPrice, NewCopyTradingParams, NewCounterTradeParams, NewMarketParams,
        },
    },
    namespace::{COPY_TRADING_LAST_ADDED, FACTORY_MARKET_LAST_ADDED},
    prelude::FactoryExecuteMsg,
    storage::{FactoryQueryMsg, MarketId},
    time::Timestamp,
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
                spot_price: perpswap::contracts::market::spot_price::SpotPriceConfigInit::Manual {
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
fn test_factory_sudo_fail_with_owner() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.set_time(TimeJump::Hours(10)).unwrap();
    market
        .sudo_factory(&FactoryExecuteMsg::AddMarket {
            new_market: NewMarketParams {
                market_id: MarketId::new(
                    "BTC",
                    "USD",
                    perpswap::storage::MarketType::CollateralIsQuote,
                ),
                token: market.token.clone().into(),
                config: None,
                spot_price: perpswap::contracts::market::spot_price::SpotPriceConfigInit::Manual {
                    admin: Addr::unchecked(TEST_CONFIG.protocol_owner.clone()).into(),
                },
                initial_borrow_fee_rate: "0.01".parse().unwrap(),
                initial_price: Some(InitialPrice {
                    price: "1".parse().unwrap(),
                    price_usd: "1".parse().unwrap(),
                }),
            },
        })
        .unwrap_err();
}

#[test]
fn test_factory_sudo_add_market() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // This is required to test the sudo entrypoint
    market
        .exec_factory(&FactoryExecuteMsg::RemoveOwner {})
        .unwrap();

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
        .sudo_factory(&FactoryExecuteMsg::AddMarket {
            new_market: NewMarketParams {
                market_id: MarketId::new(
                    "BTC",
                    "USD",
                    perpswap::storage::MarketType::CollateralIsQuote,
                ),
                token: market.token.clone().into(),
                config: None,
                spot_price: perpswap::contracts::market::spot_price::SpotPriceConfigInit::Manual {
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
    let trader = market.clone_trader(0).unwrap();
    market
        .sudo_factory(&FactoryExecuteMsg::RegisterReferrer {
            addr: Addr::unchecked(trader).into(),
        })
        .unwrap_err();

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

    let now = market.now();
    let key = COPY_TRADING_LAST_ADDED.as_bytes().to_vec();
    let result = market
        .query_factory_raw(Binary::new(key.clone()))
        .unwrap()
        .unwrap();
    let old_time: Timestamp = cosmwasm_std::from_json(result.as_slice()).unwrap();
    assert!(now > old_time);
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
        .query_factory(&perpswap::prelude::FactoryQueryMsg::CopyTrading {
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
        .query_factory(&perpswap::prelude::FactoryQueryMsg::CopyTradingForLeader {
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

fn get_copy_trading_addr(events: Vec<Event>) -> Attribute {
    events
        .into_iter()
        .find(|item| item.ty == "wasm-instantiate-copy-trading")
        .unwrap()
        .attributes
        .into_iter()
        .find(|item| item.key == "contract")
        .unwrap()
}

#[test]
fn instantiate_copy_trading_event() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    // We start from one because the test framework already has one
    // copy trading contract
    let response = market.exec_factory_add_copy_trading().unwrap();
    let copy_trading = get_copy_trading_addr(response.events);

    let contract = market.query_factory_copy_contracts().unwrap();
    // Index one because test framework already creates a contract
    assert_eq!(
        contract.addresses[1].contract.0.to_string(),
        copy_trading.value
    );
}

#[test]
fn query_factory_copy_trading_order_mid_query() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    // We start from one because the test framework already has one
    // copy trading contract
    let initial_response = market.query_factory_copy_contracts().unwrap();

    assert!(initial_response.addresses.len() == 1);
    let mut result = vec![];
    result.push(initial_response.addresses[0].contract.0.to_string());
    for _ in 0..=1 {
        market.exec_factory_add_copy_trading().unwrap();
    }

    let mid_response = market.query_factory_copy_contracts().unwrap();
    assert_eq!(mid_response.addresses.len(), 3);

    let cursor = mid_response
        .addresses
        .last()
        .cloned()
        .map(|item| CopyTradingInfoRaw {
            leader: item.leader.0.into(),
            contract: item.contract.0.into(),
        });

    for _ in 0..=1 {
        let response = market.exec_factory_add_copy_trading().unwrap();
        let addr = get_copy_trading_addr(response.events);
        result.push(addr.value);
    }

    let res: CopyTradingResp = market
        .query_factory(&FactoryQueryMsg::CopyTrading {
            start_after: cursor,
            limit: None,
        })
        .unwrap();

    let response = market.query_factory_copy_contracts().unwrap();
    assert_eq!(response.addresses.len(), 5);

    assert_eq!(res.addresses.len(), 2);
}

#[test]
fn query_factory_copy_trading_order() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    // We start from one because the test framework already has one
    // copy trading contract
    let initial_response = market.query_factory_copy_contracts().unwrap();

    assert!(initial_response.addresses.len() == 1);
    let mut result = vec![];
    result.push(initial_response.addresses[0].contract.0.to_string());
    for _ in 0..5 {
        let response = market.exec_factory_add_copy_trading().unwrap();
        let addr = get_copy_trading_addr(response.events);
        result.push(addr.value);
    }

    let final_response = market.query_factory_copy_contracts().unwrap();
    let final_response = final_response
        .addresses
        .iter()
        .map(|item| item.contract.0.to_string())
        .collect::<Vec<_>>();

    assert_eq!(final_response, result);
}

#[test]
fn copy_trading_timestamp_updated() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let resp = market.query_factory_copy_contracts().unwrap();
    assert!(resp.addresses.len() == 1);
    assert_eq!(market.copy_trading_addr, resp.addresses[0].contract.0);

    let now = market.now();
    let key = COPY_TRADING_LAST_ADDED.as_bytes().to_vec();
    let result = market
        .query_factory_raw(Binary::new(key.clone()))
        .unwrap()
        .unwrap();
    let old_time: Timestamp = cosmwasm_std::from_json(result.as_slice()).unwrap();
    assert!(now > old_time);

    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_factory_add_copy_trading().unwrap();

    let result = market
        .query_factory_raw(Binary::new(key.clone()))
        .unwrap()
        .unwrap();
    let last_updated_time: Timestamp = cosmwasm_std::from_json(result.as_slice()).unwrap();
    assert_ne!(old_time, last_updated_time);
    assert!(last_updated_time > old_time);
}

#[test]
fn non_admin_add_counter_trade_contract() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let btc_usd = MarketId::new(
        "BTC",
        "USD",
        perpswap::storage::MarketType::CollateralIsQuote,
    );

    market
        .exec_factory(&FactoryExecuteMsg::AddMarket {
            new_market: NewMarketParams {
                market_id: btc_usd.clone(),
                token: market.token.clone().into(),
                config: None,
                spot_price: perpswap::contracts::market::spot_price::SpotPriceConfigInit::Manual {
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

    let lp = market.clone_lp(0).unwrap();
    market
        .exec_factory_as(
            &lp,
            &FactoryExecuteMsg::AddCounterTrade {
                new_counter_trade: NewCounterTradeParams { market_id: btc_usd },
            },
        )
        .unwrap();
}

#[test]
fn admin_add_counter_trade_contract() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let query: CounterTradeResp = market
        .query_factory(&FactoryQueryMsg::CounterTrade {
            start_after: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(query.addresses.len(), 1);

    let btc_usd = MarketId::new(
        "BTC",
        "USD",
        perpswap::storage::MarketType::CollateralIsQuote,
    );

    market
        .exec_factory(&FactoryExecuteMsg::AddMarket {
            new_market: NewMarketParams {
                market_id: btc_usd.clone(),
                token: market.token.clone().into(),
                config: None,
                spot_price: perpswap::contracts::market::spot_price::SpotPriceConfigInit::Manual {
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

    market
        .exec_factory(&FactoryExecuteMsg::AddCounterTrade {
            new_counter_trade: NewCounterTradeParams { market_id: btc_usd },
        })
        .unwrap();

    let query: CounterTradeResp = market
        .query_factory(&FactoryQueryMsg::CounterTrade {
            start_after: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(query.addresses.len(), 2);
    assert_ne!(query.addresses[0].contract, query.addresses[1].contract);
}

#[test]
fn multiple_same_market_id_counter_trade_contract() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_id = market.id.clone();

    market
        .exec_factory(&FactoryExecuteMsg::AddCounterTrade {
            new_counter_trade: NewCounterTradeParams { market_id },
        })
        .unwrap_err();
}
