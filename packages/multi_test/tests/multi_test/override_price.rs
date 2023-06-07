use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{contracts::market::entry::PriceForQuery, prelude::*};

#[test]
fn status() {
    // Set up a market that does _not_ have USD as notional.
    let market = PerpsMarket::new_custom(
        PerpsApp::new_cell().unwrap(),
        "WBTC_BTC".parse().unwrap(),
        msg::token::TokenInit::Native {
            denom: "BTC".to_owned(),
        },
        "50".parse().unwrap(),
        Some("50".parse().unwrap()),
        true,
    )
    .unwrap();

    let trader = market.clone_trader(0).unwrap();

    market
        .exec_open_position(
            &trader,
            "5",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();
    market
        .exec_open_position(
            &trader,
            "5",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let status1 = market.query_status().unwrap();
    let status2 = market
        .query_status_with_price(PriceForQuery {
            base: "42".parse().unwrap(),
            collateral: Some("42".parse().unwrap()),
        })
        .unwrap();

    assert_eq!(status1.long_notional, status2.long_notional);
    assert_eq!(status1.short_notional, status2.short_notional);
    assert_ne!(status1.long_usd, status2.long_usd);
    assert_ne!(status1.short_usd, status2.short_usd);
}

#[test]
fn positions() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market.exec_set_price("100".parse().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();

    let (long, _) = market
        .exec_open_position(
            &trader,
            "5",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();
    let (short, _) = market
        .exec_open_position(
            &trader,
            "5",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    todo!()
}

#[test]
fn lp_info() {
    todo!()
}

#[test]
fn dnf() {
    todo!()
}
