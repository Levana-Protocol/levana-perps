mod helper;

use cosmwasm_std::{Addr, Decimal256, StdError};
use helper::{
    exec_set_oracle_price_base, exec_set_oracle_price_usd, setup_test_env, MockOraclePriceResp,
    MockPriceResponse,
};
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::market::{
        entry::NewMarketParams,
        entry::QueryMsg,
        spot_price::SpotPriceConfigInit,
        spot_price::{SpotPriceFeed, SpotPriceFeedData},
    },
    prelude::*,
    storage::MarketId,
};

#[test]
fn test_rujira() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let eth_rune = MarketId::new(
        "ETH",
        "RUNE",
        perpswap::storage::MarketType::CollateralIsQuote,
    );
    let rujira_feed = SpotPriceFeed {
        data: SpotPriceFeedData::Rujira {
            asset: "ETH.RUNE".to_owned(),
        },
        inverted: false,
        volatile: None,
    };
    let market_addr = market
        .exec_factory(&FactoryExecuteMsg::AddMarket {
            new_market: NewMarketParams {
                market_id: eth_rune.clone(),
                token: market.token.clone().into(),
                config: None,
                spot_price: SpotPriceConfigInit::Oracle {
                    pyth: None,
                    stride: None,
                    feeds: vec![rujira_feed.clone().into()],
                    feeds_usd: vec![rujira_feed.clone().into()],
                    volatile_diff_seconds: None,
                },
                initial_borrow_fee_rate: "0.01".parse().unwrap(),
                initial_price: None,
            },
        })
        .unwrap()
        .events
        .iter()
        .find(|e| e.ty == "instantiate")
        .context("could not instantiate")
        .unwrap()
        .attributes
        .iter()
        .find(|a| a.key == "_contract_address")
        .context("could not find contract_address")
        .unwrap()
        .value
        .clone();

    market.addr = Addr::unchecked(market_addr);
    market.id = eth_rune;

    market.exec_refresh_price().unwrap();

    let price = market.query_current_price().unwrap();
    println!("{:?}", price);
}

#[test]
fn test_oracle_price_valid() {
    let (mut app, market_addr) = setup_test_env(MockPriceResponse::Valid);

    let query_msg = QueryMsg::OraclePrice { validate_age: true };

    let res: Result<MockOraclePriceResp, StdError> =
        app.wrap().query_wasm_smart(&market_addr, &query_msg);

    assert!(res.is_ok(), "Oracle price query should succeed: {:?}", res);

    let price_usd = Decimal256::from_str("2").unwrap();
    let timestamp = Some(Uint64::from(1234567890u64));
    let x = exec_set_oracle_price_usd(&mut app, &market_addr, price_usd, timestamp).unwrap();

    println!("{:?}", x);

    let price_base = Decimal256::from_str("50").unwrap();
    let timestamp = Some(Uint64::from(1234567890u64));
    let x = exec_set_oracle_price_base(&mut app, &market_addr, price_base, timestamp).unwrap();
    println!("{:?}", x);

    let resp = res.unwrap();

    assert_eq!(
        resp.rujira.get("ETH.RUNE").unwrap().price,
        Decimal256::from_str("10.0").unwrap(),
        "Invalid price returned"
    );
}

#[test]
fn test_oracle_price_zero() {
    let (mut app, market_addr) = setup_test_env(MockPriceResponse::Zero);

    let query_msg = QueryMsg::OraclePrice { validate_age: true };

    let _res: Result<MockOraclePriceResp, StdError> =
        app.wrap().query_wasm_smart(&market_addr, &query_msg);

    let price_usd = Decimal256::from_str("40").unwrap();
    let timestamp = Some(Uint64::from(1234567890u64));
    let x = exec_set_oracle_price_usd(&mut app, &market_addr, price_usd, timestamp).unwrap();
    println!("{:?}", x);

    let res: Result<MockOraclePriceResp, StdError> =
        app.wrap().query_wasm_smart(&market_addr, &query_msg);

    println!("{:?}", res);

    let price_base = Decimal256::from_str("0").unwrap();
    let timestamp = Some(Uint64::from(1234567890u64));
    let x = exec_set_oracle_price_base(&mut app, &market_addr, price_base, timestamp).unwrap();

    println!("{:?}", x);
    // This should be an error, but we are still exploring this
    //assert!(res.is_err(), "Oracle price query should fail");
}

#[test]
fn test_oracle_price_nan() {
    let (mut app, market_addr) = setup_test_env(MockPriceResponse::NaN);

    let query_msg = QueryMsg::OraclePrice { validate_age: true };

    let res: Result<MockOraclePriceResp, StdError> =
        app.wrap().query_wasm_smart(&market_addr, &query_msg);
    println!("{:?}", res);

    let price_usd = Decimal256::from_str("50").unwrap();
    let timestamp = Some(Uint64::from(1234567890u64));
    let x = exec_set_oracle_price_usd(&mut app, &market_addr, price_usd, timestamp).unwrap();
    println!("{:?}", x);

    let res: Result<MockOraclePriceResp, StdError> =
        app.wrap().query_wasm_smart(&market_addr, &query_msg);
    println!("{:?}", res);
    // Error parsing NaN
    assert!(res.is_err(), "Oracle price query should fail");
}

