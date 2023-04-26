use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::prelude::*;

#[test]
fn auth_price_admin() {
    let app = PerpsApp::new_cell().unwrap();
    let market_1 = PerpsMarket::new(app.clone()).unwrap();
    let market_2 = PerpsMarket::new_custom(
        app,
        MarketId::new("BTC", "EUR", MarketType::CollateralIsBase),
        market_1.token.clone().into(),
        PriceBaseInQuote::try_from_number(Number::ONE).unwrap(),
        Some("1.5".parse().unwrap()),
        true,
    )
    .unwrap();

    let price_admin_1 = Addr::unchecked("price_admin_1");
    let price_admin_2 = Addr::unchecked("price_admin_2");
    let price_admin_3 = Addr::unchecked("price_admin_3");

    // do not go through the market helper
    // so that we can use a specific admin and catch errors
    let set_price = |market: &PerpsMarket, admin: &Addr| {
        market.exec(
            admin,
            &msg::contracts::market::entry::ExecuteMsg::SetPrice {
                price: PriceBaseInQuote::try_from_number(Number::from(1u64)).unwrap(),
                price_usd: Some(PriceCollateralInUsd::try_from_number(Number::ONE).unwrap()),
                execs: Some(0),
                rewards: None,
            },
        )
    };

    // Test updating admin on a single market
    market_1
        .exec_set_admin_for_price_updates(&price_admin_1)
        .unwrap();
    set_price(&market_1, &price_admin_1).unwrap();
    set_price(&market_1, &price_admin_2).unwrap_err();
    set_price(&market_1, &price_admin_3).unwrap_err();

    market_1
        .exec_set_admin_for_price_updates(&price_admin_2)
        .unwrap();
    set_price(&market_1, &price_admin_1).unwrap_err();
    set_price(&market_1, &price_admin_2).unwrap();
    set_price(&market_1, &price_admin_3).unwrap_err();

    market_1
        .exec_set_admin_for_price_updates(&price_admin_1)
        .unwrap();
    set_price(&market_1, &price_admin_1).unwrap();
    set_price(&market_1, &price_admin_2).unwrap_err();
    set_price(&market_1, &price_admin_3).unwrap_err();

    // Test that it's unaffected by multiple markets

    market_2
        .exec_set_admin_for_price_updates(&price_admin_1)
        .unwrap();
    set_price(&market_1, &price_admin_1).unwrap();
    set_price(&market_1, &price_admin_2).unwrap_err();
    set_price(&market_1, &price_admin_3).unwrap_err();
    set_price(&market_2, &price_admin_1).unwrap();
    set_price(&market_2, &price_admin_2).unwrap_err();
    set_price(&market_2, &price_admin_3).unwrap_err();

    market_2
        .exec_set_admin_for_price_updates(&price_admin_2)
        .unwrap();
    set_price(&market_1, &price_admin_1).unwrap();
    set_price(&market_1, &price_admin_2).unwrap_err();
    set_price(&market_1, &price_admin_3).unwrap_err();
    set_price(&market_2, &price_admin_1).unwrap_err();
    set_price(&market_2, &price_admin_2).unwrap();
    set_price(&market_2, &price_admin_3).unwrap_err();
}
