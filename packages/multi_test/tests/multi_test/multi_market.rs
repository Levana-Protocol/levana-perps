use levana_perpswap_multi_test::{config::DEFAULT_MARKET, market_wrapper::PerpsMarket, PerpsApp};
use msg::prelude::*;

#[test]
fn multi_market() {
    let app = PerpsApp::new_cell().unwrap();
    let market_1 = PerpsMarket::new(app.clone()).unwrap();
    let market_2 = PerpsMarket::new_custom(
        app,
        MarketId::new("BTC", "EUR", MarketType::CollateralIsBase),
        market_1.token.clone().into(),
        PriceBaseInQuote::try_from_number(Number::ONE).unwrap(),
        Some("1".parse().unwrap()),
        None,
        true,
        DEFAULT_MARKET.spot_price,
    )
    .unwrap();

    let stats = market_1.query_crank_stats().unwrap();
    println!("stats for market {}\n {:#?}", market_1.id, stats);
    let stats = market_2.query_crank_stats().unwrap();
    println!("stats for market {}\n {:#?}", market_2.id, stats);
}
