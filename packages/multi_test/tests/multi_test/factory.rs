use cosmwasm_std::{Addr, Binary};
use levana_perpswap_multi_test::{
    config::TEST_CONFIG, market_wrapper::PerpsMarket, time::TimeJump, PerpsApp,
};
use msg::{
    contracts::market::entry::{InitialPrice, NewMarketParams},
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
