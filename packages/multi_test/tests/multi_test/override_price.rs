use levana_perpswap_multi_test::{config::DEFAULT_MARKET, market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::market::{
        entry::PriceForQuery,
        position::{LiquidationReason, PositionCloseReason},
    },
    prelude::*,
};

#[test]
fn status() {
    // Set up a market that does _not_ have USD as notional.
    let market = PerpsMarket::new_custom(
        PerpsApp::new_cell().unwrap(),
        "WBTC_BTC".parse().unwrap(),
        msg::token::TokenInit::Native {
            denom: "BTC".to_owned(),
            decimal_places: 8,
        },
        "50".parse().unwrap(),
        Some("50".parse().unwrap()),
        None,
        true,
        DEFAULT_MARKET.spot_price,
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

    let new_price = PriceForQuery {
        base: "42".parse().unwrap(),
        collateral: "42".parse().unwrap(),
    };

    let status1 = market.query_status().unwrap();
    let status2 = market.query_status_with_price(new_price).unwrap();

    assert_eq!(status1.long_notional, status2.long_notional);
    assert_eq!(status1.short_notional, status2.short_notional);

    let market_type = market.id.get_market_type();
    let price_point = PricePoint {
        price_notional: new_price.base.into_notional_price(market_type),
        price_usd: new_price.collateral,
        price_base: new_price.base,
        timestamp: market.now(),
        is_notional_usd: market.id.is_notional_usd(),
        market_type,
        publish_time: None,
        publish_time_usd: None,
    };

    assert_ne!(status1.long_usd, status2.long_usd);
    assert_ne!(status1.short_usd, status2.short_usd);
    assert_eq!(
        price_point.notional_to_usd(status1.long_notional),
        status2.long_usd
    );
    assert_eq!(
        price_point.notional_to_usd(status1.short_notional),
        status2.short_usd
    );
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

    let longreal = market.query_position(long).unwrap();
    let shortreal = market.query_position(short).unwrap();

    let pricehigh = PriceForQuery::from_usd_market("101".parse().unwrap(), &market.id).unwrap();
    let longhigh = market.query_position_with_price(long, pricehigh).unwrap();
    let shorthigh = market.query_position_with_price(short, pricehigh).unwrap();
    assert!(longhigh.pnl_collateral > longreal.pnl_collateral);
    assert!(shorthigh.pnl_collateral < shortreal.pnl_collateral);

    let pricelow = PriceForQuery::from_usd_market("99".parse().unwrap(), &market.id).unwrap();
    let longlow = market.query_position_with_price(long, pricelow).unwrap();
    let shortlow = market.query_position_with_price(short, pricelow).unwrap();
    assert!(longlow.pnl_collateral < longreal.pnl_collateral);
    assert!(shortlow.pnl_collateral > shortreal.pnl_collateral);

    market.exec_set_price("99".parse().unwrap()).unwrap();
    let longfinal = market.query_position(long).unwrap();
    let shortfinal = market.query_position(short).unwrap();

    assert_eq!(longlow.pnl_collateral, longfinal.pnl_collateral);
    assert_eq!(shortlow.pnl_collateral, shortfinal.pnl_collateral);
}

#[test]
fn liquidate_positions() {
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

    let longreal = market.query_position(long).unwrap();
    let shortreal = market.query_position(short).unwrap();

    let pricehigh = PriceForQuery::from_usd_market("200".parse().unwrap(), &market.id).unwrap();
    let longhigh = market
        .query_position_pending_close_with_price(long, pricehigh)
        .unwrap();
    let shorthigh = market
        .query_position_pending_close_with_price(short, pricehigh)
        .unwrap();
    assert_eq!(
        longhigh.reason,
        PositionCloseReason::Liquidated(LiquidationReason::MaxGains)
    );
    assert_eq!(
        shorthigh.reason,
        PositionCloseReason::Liquidated(LiquidationReason::Liquidated)
    );
    assert!(longhigh.pnl_collateral > longreal.pnl_collateral);
    assert!(shorthigh.pnl_collateral < shortreal.pnl_collateral);

    let pricelow = PriceForQuery::from_usd_market("50".parse().unwrap(), &market.id).unwrap();
    let longlow = market
        .query_position_pending_close_with_price(long, pricelow)
        .unwrap();
    let shortlow = market
        .query_position_pending_close_with_price(short, pricelow)
        .unwrap();
    assert_eq!(
        longlow.reason,
        PositionCloseReason::Liquidated(LiquidationReason::Liquidated)
    );
    assert_eq!(
        shortlow.reason,
        PositionCloseReason::Liquidated(LiquidationReason::MaxGains)
    );
    assert!(longlow.pnl_collateral < longreal.pnl_collateral);
    assert!(shortlow.pnl_collateral > shortreal.pnl_collateral);
}

#[test]
fn would_trigger() {
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
    let (_short, _) = market
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

    let pricehigh = "101".parse().unwrap();
    let priceveryhigh = "200".parse().unwrap();
    let pricelow = "99".parse().unwrap();
    let priceverylow = "50".parse().unwrap();

    assert!(!market.query_price_would_trigger(pricehigh).unwrap());
    assert!(!market.query_price_would_trigger(pricelow).unwrap());
    assert!(market.query_price_would_trigger(priceveryhigh).unwrap());
    assert!(market.query_price_would_trigger(priceverylow).unwrap());

    // Ensure that both liquidations and take profits are working by closing the long and only testing the short

    market.exec_close_position(&trader, long, None).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    assert!(!market.query_price_would_trigger(pricehigh).unwrap());
    assert!(!market.query_price_would_trigger(pricelow).unwrap());
    assert!(market.query_price_would_trigger(priceveryhigh).unwrap(),);
    assert!(market.query_price_would_trigger(priceverylow).unwrap(),);
}

#[test]
fn would_trigger_on_limit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market.exec_set_price("100".parse().unwrap()).unwrap();
    assert!(!market
        .query_price_would_trigger("90".parse().unwrap())
        .unwrap());

    let trader = market.clone_trader(0).unwrap();

    let (long, _) = market
        .exec_place_limit_order(
            &trader,
            "5".parse().unwrap(),
            "90".parse().unwrap(),
            "10".parse().unwrap(),
            DirectionToBase::Long,
            "1.0".parse().unwrap(),
            None,
            None,
        )
        .unwrap();
    assert!(market
        .query_price_would_trigger("90".parse().unwrap())
        .unwrap());
    assert!(market
        .query_price_would_trigger("89".parse().unwrap())
        .unwrap());
    assert!(!market
        .query_price_would_trigger("91".parse().unwrap())
        .unwrap());

    market.exec_cancel_limit_order(&trader, long).unwrap();
    assert!(!market
        .query_price_would_trigger("90".parse().unwrap())
        .unwrap());
    assert!(!market
        .query_price_would_trigger("89".parse().unwrap())
        .unwrap());
    assert!(!market
        .query_price_would_trigger("91".parse().unwrap())
        .unwrap());
}
