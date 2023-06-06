use levana_perpswap_multi_test::position_helpers::{
    assert_position_max_gains, assert_position_stop_loss,
};
use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, position_helpers::assert_position_liquidated, time::TimeJump,
    PerpsApp,
};
use msg::contracts::market::config::ConfigUpdate;
use msg::contracts::market::crank::CrankWorkInfo;
use msg::contracts::market::position::PositionId;
use msg::prelude::*;

#[test]
fn position_liquidate_delayed_crank() {
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
            "10",
            DirectionToBase::Long,
            "1.0",
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

    // set price below liquidation point - do not crank
    market.exec_set_price("0.5".try_into().unwrap()).unwrap();

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
    assert_position_liquidated(&pos).unwrap();
}

#[test]
fn position_liquidate_long() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "0.5",
            None,
            None,
            None,
        )
        .unwrap();

    market.exec_set_price("1".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();

    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert_position_liquidated(&pos).unwrap();

    // confirm that we get no positions when we query
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn position_liquidate_short() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();

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

    market.exec_set_price("10".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();

    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert_position_max_gains(&pos).unwrap();

    // confirm that we get no positions when we query
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn position_stop_loss_long() {
    // Setup

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();
    let stop_loss_override = PriceBaseInQuote::try_from_number(95u128.into()).unwrap();
    let trigger_and_assert = |pos_id: PositionId| {
        market.exec_set_price("95".try_into().unwrap()).unwrap();
        market.exec_crank(&cranker).unwrap();

        let pos = market.query_closed_position(&trader, pos_id).unwrap();
        assert_position_stop_loss(&pos).unwrap();
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
            Some(stop_loss_override),
            None,
        )
        .unwrap();

    trigger_and_assert(pos_id);

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
        .exec_set_trigger_order(&trader, pos_id, Some(stop_loss_override), None)
        .unwrap();

    trigger_and_assert(pos_id);
}

#[test]
fn position_stop_loss_short() {
    // Setup

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();
    let stop_loss_override = PriceBaseInQuote::try_from_number(105u128.into()).unwrap();
    let trigger_and_assert = |pos_id: PositionId| {
        market.exec_set_price("105".try_into().unwrap()).unwrap();
        market.exec_crank(&cranker).unwrap();

        let pos = market.query_closed_position(&trader, pos_id).unwrap();
        assert_position_stop_loss(&pos).unwrap();
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
            Some(stop_loss_override),
            None,
        )
        .unwrap();

    trigger_and_assert(pos_id);

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
        .exec_set_trigger_order(&trader, pos_id, Some(stop_loss_override), None)
        .unwrap();

    trigger_and_assert(pos_id);
}

#[test]
fn position_liquidate_long_same_block() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "0.5",
            None,
            None,
            None,
        )
        .unwrap();

    // make sure cranking is all caught up
    market.exec_crank_till_finished(&cranker).unwrap();

    // update price and crank in the same block
    market.automatic_time_jump_enabled = false;
    market.exec_set_price("1".try_into().unwrap()).unwrap();
    let res = market.exec_crank_till_finished(&cranker).unwrap();

    let crank_liquidation_price_point: PricePoint = res
        .into_iter()
        .flat_map(|res| res.events)
        .find_map(|event| {
            if let Ok(crank_work) = CrankWorkInfo::try_from(event) {
                match crank_work {
                    CrankWorkInfo::Liquidation { price_point, .. } => Some(price_point),
                    _ => None,
                }
            } else {
                None
            }
        })
        .unwrap();

    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert_position_liquidated(&pos).unwrap();

    // confirm that the position was settled up to the liquidation time
    assert!(pos.settlement_time >= crank_liquidation_price_point.timestamp);

    // confirm that we get no positions when we query
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}
