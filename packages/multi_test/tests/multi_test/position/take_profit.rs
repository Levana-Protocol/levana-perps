use levana_perpswap_multi_test::position_helpers::{
    assert_position_liquidated, assert_position_liquidated_reason,
};
use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, position_helpers::assert_position_max_gains, time::TimeJump,
    PerpsApp,
};
use msg::contracts::market::config::ConfigUpdate;
use msg::contracts::market::position::{LiquidationReason, PositionId};
use msg::prelude::*;

#[test]
fn position_take_profit_long_normal() {
    position_take_profit_long_helper(10, true);
}

#[test]
fn position_take_profit_long_massive() {
    position_take_profit_long_helper(1000, false);
}

#[test]
fn position_take_profit_short() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("1".try_into().unwrap()).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "0.5",
            None,
            None,
            None,
        )
        .unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();

    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert_position_liquidated(&pos).unwrap();

    // confirm that we get no positions when we query
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn position_take_profit_delayed_crank() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market
        .exec_set_config(ConfigUpdate {
            minimum_deposit_usd: Some("0".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    let (pos_1, _) = market
        .exec_open_position(
            &trader,
            "5",
            "2",
            DirectionToBase::Long,
            "0.2",
            None,
            None,
            None,
        )
        .unwrap();

    market.exec_crank(&cranker).unwrap();

    // jump forward to part of an epoch - do not crank
    market
        .set_time(TimeJump::FractionalLiquifundings(0.25))
        .unwrap();

    // set price below liquidation point
    market.exec_set_price("1.5".try_into().unwrap()).unwrap();

    // jump forward the rest of the epoch - do not crank
    market
        .set_time(TimeJump::FractionalLiquifundings(0.75))
        .unwrap();
    // gotta refresh the price here or else market will be stale
    market.exec_refresh_price().unwrap();

    // open another position
    let (_, _) = market
        .exec_open_position(
            &trader,
            "3",
            "3",
            DirectionToBase::Long,
            "0.3",
            None,
            None,
            None,
        )
        .unwrap();

    // jump forward to cross the epoch change
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    // now finally crank it all
    market.exec_crank_till_finished(&cranker).unwrap();

    // confirm that the position is closed
    let pos = market.query_closed_position(&trader, pos_1).unwrap();
    assert_position_max_gains(&pos).unwrap();
}

#[test]
fn position_take_profit_override_long() {
    // Setup

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();
    let take_profit_override = PriceBaseInQuote::try_from_number(105u128.into()).unwrap();
    let trigger_and_assert = |pos_id: PositionId, reason: LiquidationReason| {
        market.exec_set_price("105".try_into().unwrap()).unwrap();
        market.exec_crank(&cranker).unwrap();

        let pos = market.query_closed_position(&trader, pos_id).unwrap();
        assert_position_liquidated_reason(&pos, reason).unwrap();
    };

    // Test setting stop loss override in open position msg

    market.exec_set_price("100".try_into().unwrap()).unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            Some(take_profit_override),
        )
        .unwrap();

    trigger_and_assert(pos_id, LiquidationReason::MaxGains);

    // Test setting stop loss override via set msg

    market.exec_set_price("100".try_into().unwrap()).unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_set_trigger_order(&trader, pos_id, None, Some(take_profit_override))
        .unwrap();

    trigger_and_assert(pos_id, LiquidationReason::TakeProfit);
}

#[test]
fn position_take_profit_override_short() {
    // Setup

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();
    let take_profit_override = PriceBaseInQuote::try_from_number(95u128.into()).unwrap();
    let trigger_and_assert = |pos_id: PositionId, reason: LiquidationReason| {
        market.exec_set_price("95".try_into().unwrap()).unwrap();
        market.exec_crank(&cranker).unwrap();

        let pos = market.query_closed_position(&trader, pos_id).unwrap();
        assert_position_liquidated_reason(&pos, reason).unwrap();
    };

    // Test setting stop loss override in open position msg

    market.exec_set_price("100".try_into().unwrap()).unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            Some(take_profit_override),
        )
        .unwrap();

    trigger_and_assert(pos_id, LiquidationReason::MaxGains);

    // Test setting stop loss override via set msg

    market.exec_set_price("100".try_into().unwrap()).unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_set_trigger_order(&trader, pos_id, None, Some(take_profit_override))
        .unwrap();

    trigger_and_assert(pos_id, LiquidationReason::TakeProfit);
}

fn position_take_profit_long_helper(price: u128, check_liquidation_reason: bool) {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // change spot price
    market.exec_set_price("1".try_into().unwrap()).unwrap();

    let balance_before_open = market.query_collateral_balance(&trader).unwrap();
    let collateral = 100u64.into();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            collateral,
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let balance_after_open = market.query_collateral_balance(&trader).unwrap();
    assert_eq!(balance_before_open - balance_after_open, collateral);

    let pos = market.query_position(pos_id).unwrap();
    let counter_collateral = pos.counter_collateral;

    // change spot price
    market
        .exec_set_price(PriceBaseInQuote::try_from_number(price.into()).unwrap())
        .unwrap();

    // crank - which will cause a liquidation
    market.exec_crank(&cranker).unwrap();
    let pos = market.query_closed_position(&trader, pos_id).unwrap();

    let balance_after_close = market.query_collateral_balance(&trader).unwrap();

    // Extreme price movements may cause a false liquidation, since
    // active collateral cannot cover the new liquidation margin
    // In those cases, ignore the reason and confirm profits were taken
    // with the assertions below.
    // see also the SKIP_CHECK_LARGE_PNL_IS_TAKE_PROFIT flag in pnl tests
    if check_liquidation_reason {
        assert_position_max_gains(&pos).unwrap();
    }

    assert!(balance_after_close > balance_before_open);
    assert!(balance_after_close - balance_before_open <= counter_collateral.into_number());

    // confirm that we get no positions when we query
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}
