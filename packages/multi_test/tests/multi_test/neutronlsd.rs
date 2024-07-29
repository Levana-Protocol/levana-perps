use std::str::FromStr;

use levana_perpswap_multi_test::{config::DEFAULT_MARKET, market_wrapper::PerpsMarket, PerpsApp};
use msg::prelude::Number;

#[test]
fn different_collateral_price() {
    let market = PerpsMarket::new_custom(
        PerpsApp::new_cell().unwrap(),
        "wstETH_USD".parse().unwrap(),
        msg::token::TokenInit::Native {
            denom: "wstETH".to_owned(),
            decimal_places: 18,
        },
        "3000".parse().unwrap(),
        Some("6000".parse().unwrap()),
        None,
        true,
        DEFAULT_MARKET.spot_price,
    )
    .unwrap();
    let trader = market.clone_trader(0).unwrap();
    let (pos_id, _) = market
        .exec_open_position_take_profit(
            &trader,
            "1",
            "5",
            msg::prelude::DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("4000".parse().unwrap()),
        )
        .unwrap();
    market
        .exec_set_price_with_usd("5000".parse().unwrap(), Some("6000".parse().unwrap()))
        .unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    let pos_close = market.query_closed_position(&trader, pos_id).unwrap();

    // We don't need precision in these tests, let's make sure we're in the right ballpark.
    assert!(
        pos_close.pnl_collateral >= "0.99".parse().unwrap(),
        "Unexpected PnL: {}",
        pos_close.pnl_collateral
    );
    assert!(pos_close.pnl_collateral < "1".parse().unwrap());
    assert_eq!(
        (pos_close.pnl_collateral.into_number() * Number::from_str("6000").unwrap()).unwrap(),
        pos_close.pnl_usd.into_number()
    );
}

#[test]
fn min_collateral() {
    let market = PerpsMarket::new_custom(
        PerpsApp::new_cell().unwrap(),
        "stATOM_USD".parse().unwrap(),
        msg::token::TokenInit::Native {
            denom: "stATOM".to_owned(),
            decimal_places: 6,
        },
        "3".parse().unwrap(),
        Some("6".parse().unwrap()),
        None,
        true,
        DEFAULT_MARKET.spot_price,
    )
    .unwrap();
    let trader = market.clone_trader(0).unwrap();
    market
        .exec_open_position_take_profit(
            &trader,
            "1",
            "5",
            msg::prelude::DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::PosInfinity,
        )
        .unwrap();

    market
        .exec_set_price_with_usd("3".parse().unwrap(), Some("3.5".parse().unwrap()))
        .unwrap();
    market
        .exec_open_position_take_profit(
            &trader,
            "1",
            "5",
            msg::prelude::DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::PosInfinity,
        )
        .unwrap_err();
}

#[test]
fn collateral_price_doesnt_liquidate() {
    let market = PerpsMarket::new_custom(
        PerpsApp::new_cell().unwrap(),
        "wstETH_USD".parse().unwrap(),
        msg::token::TokenInit::Native {
            denom: "wstETH".to_owned(),
            decimal_places: 18,
        },
        "3000".parse().unwrap(),
        Some("6000".parse().unwrap()),
        None,
        true,
        DEFAULT_MARKET.spot_price,
    )
    .unwrap();
    let trader = market.clone_trader(0).unwrap();
    let (pos_id, _) = market
        .exec_open_position_take_profit(
            &trader,
            "1",
            "5",
            msg::prelude::DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("4000".parse().unwrap()),
        )
        .unwrap();
    market
        .exec_set_price_with_usd("3000".parse().unwrap(), Some("12000".parse().unwrap()))
        .unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    market.query_closed_position(&trader, pos_id).unwrap_err();
}
