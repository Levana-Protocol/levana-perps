use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, position_helpers::assert_position_liquidated,
    return_unless_market_collateral_base, time::TimeJump, PerpsApp,
};
use msg::{contracts::market::config::ConfigUpdate, prelude::*};

#[test]
fn liquidation_price() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market
        .exec_set_config(ConfigUpdate {
            trading_fee_notional_size: Some("0.001".parse().unwrap()),
            trading_fee_counter_collateral: Some("0.001".parse().unwrap()),
            delta_neutrality_fee_tax: Some(Decimal256::zero()),
            ..Default::default()
        })
        .unwrap();

    let trader = market.clone_trader(0).unwrap();

    market.set_time(TimeJump::Seconds(3)).unwrap();
    market.exec_set_price("10".try_into().unwrap()).unwrap();

    let initial_collateral = Collateral::from_str("100").unwrap();
    let initial_leverage = Number::from(10u64);
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            initial_collateral.into_number(),
            initial_leverage.to_string().as_str(),
            DirectionToBase::Long,
            "10",
            None,
            None,
            None,
        )
        .unwrap();

    let position_data = market.query_position(pos_id).unwrap();

    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            assert_eq!(
                position_data.trading_fee_collateral,
                Collateral::from_str("2").unwrap(),
                "trading fee is miscalculated"
            );
            assert_eq!(
                position_data.notional_size,
                Signed::<Notional>::from_str("100").unwrap(),
                "notional size is miscalculated"
            );
            assert_eq!(
                position_data.notional_size_in_collateral,
                Signed::<Collateral>::from_str("1000").unwrap(),
                "notional value is miscalculated"
            );
            assert!(
                position_data
                    .leverage
                    .into_number()
                    .approx_eq_eps("10.20418576".parse().unwrap(), Number::EPS_E6),
                "leverage is miscalculated"
            );
        }
        MarketType::CollateralIsBase => {
            assert_eq!(
                position_data.trading_fee_collateral,
                Collateral::from_str("1.35").unwrap(),
                "trading fee is miscalculated"
            );
            assert_eq!(
                position_data.notional_size,
                Signed::<Notional>::from_str("-9000").unwrap(),
                "notional size is miscalculated"
            );
            assert_eq!(
                position_data.notional_size_in_collateral,
                Signed::<Collateral>::from_str("-900").unwrap(),
                "notional value is miscalculated"
            );
            assert!(
                position_data
                    .leverage
                    .into_number()
                    .approx_eq_eps("10.13065974".parse().unwrap(), Number::EPS_E6),
                "leverage is miscalculated"
            );
        }
    };
}

#[test]
fn liquidation_price_updates_perp_874() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "10",
            "30",
            DirectionToBase::Short,
            "3",
            None,
            None,
            None,
        )
        .unwrap();

    // Determine the initial liquidation price
    let pos1 = market.query_position(pos_id).unwrap();
    let liquidation_price1 = pos1.liquidation_price_base.unwrap();

    // Jump ahead some time to accrue fees. Get the new liquidation price, which
    // should be lower.
    market.set_time(TimeJump::Liquifundings(30)).unwrap();
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    let pos2 = market.query_position(pos_id).unwrap();
    let liquidation_price2 = pos2.liquidation_price_base.unwrap();
    assert!(liquidation_price1.into_number() > liquidation_price2.into_number(), "First liquidation price: {liquidation_price1}. Second liquidation price: {liquidation_price2}.");

    // Now set the liquidation price to the second price point plus a tiny
    // amount. It should trigger a liquidation.
    let new_price: PriceBaseInQuote = (liquidation_price2.into_number()
        + "0.0001".parse().unwrap())
    .to_string()
    .parse()
    .unwrap();
    market.exec_set_price(new_price).unwrap();
    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    // The position should have been liquidated
    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert_position_liquidated(&pos).unwrap();
}

#[test]
fn deposit_collateral_stops_liquidation_perp_874() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "10",
            "30",
            DirectionToBase::Short,
            "2",
            None,
            None,
            None,
        )
        .unwrap();

    // Get the initial liquidation point for the position
    let pos1 = market.query_position(pos_id).unwrap();
    let liquidation_price1 = pos1.liquidation_price_base.unwrap();

    // Now deposit more collateral to reduce leverage and increase the liquidation price
    market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "100".parse().unwrap())
        .unwrap();
    let pos2 = market.query_position(pos_id).unwrap();
    let liquidation_price2 = pos2.liquidation_price_base.unwrap();
    assert!(liquidation_price1.into_number() < liquidation_price2.into_number(), "First liquidation price: {liquidation_price1}. Second liquidation price: {liquidation_price2}.");

    // Now set the liquidation price to the first price point plus a tiny
    // amount. It should _not_ trigger a liquidation.
    let new_price: PriceBaseInQuote = (liquidation_price1.into_number()
        + "0.000001".parse().unwrap())
    .to_string()
    .parse()
    .unwrap();
    market.exec_set_price(new_price).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    // The position should be open
    market.query_position(pos_id).unwrap();
}

#[test]
fn pnl_from_liquidation_perp_1404() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    return_unless_market_collateral_base!(&market);

    market
        .exec_set_config(ConfigUpdate {
            liquifunding_delay_seconds: Some(60 * 60 * 24),
            liquifunding_delay_fuzz_seconds: Some(60 * 60 * 4),
            ..Default::default()
        })
        .unwrap();

    market.exec_set_price("6.33".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "17.5",
            DirectionToBase::Long,
            "+Inf",
            None,
            None,
            None,
        )
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();

    market.exec_crank_till_finished(&trader).unwrap();
    market
        .exec_set_price_and_crank(pos.liquidation_price_base.unwrap())
        .unwrap();

    let closed = market.query_closed_position(&trader, pos_id).unwrap();

    let additional_pnl = closed.pnl_collateral - pos.pnl_collateral;
    assert!(
        additional_pnl < "-0.1".parse().unwrap(),
        "Didn't lose more money from price movement. Old PnL: {}. New PnL: {}. Additional PnL: {}.",
        pos.pnl_collateral,
        closed.pnl_collateral,
        additional_pnl
    );
}
