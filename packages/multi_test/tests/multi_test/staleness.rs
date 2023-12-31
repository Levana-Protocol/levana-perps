use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, position_helpers::assert_position_liquidated, time::TimeJump,
    PerpsApp,
};
use msg::prelude::*;

#[test]
fn lagging_crank_liquidations() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // Create a huge price jump that isn't cranked
    market.set_time(TimeJump::Hours(1)).unwrap();
    market.exec_set_price("10".parse().unwrap()).unwrap();

    // Undo the huge price jump and don't crank this either
    market.set_time(TimeJump::Hours(1)).unwrap();
    market.exec_set_price("1".parse().unwrap()).unwrap();

    // No let the trader open a short position. The first price jump would
    // trigger the liquidation of this position, but shouldn't occur because
    // that price is not applicable.
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "10",
            "25",
            DirectionToBase::Short,
            "3",
            None,
            None,
            None,
        )
        .unwrap();
    market.query_position(pos_id).unwrap();

    // Now run the crank and ensure we didn't get liquidated
    market.exec_crank_till_finished(&cranker).unwrap();
    market.query_position(pos_id).unwrap();

    // And finally move the price back to the ridiculous price, crank, and ensure we _are_ liquidated
    market.set_time(TimeJump::Hours(1)).unwrap();
    market.exec_set_price("10".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();
    market.query_position(pos_id).unwrap_err();
    let closed = market.query_closed_position(&trader, pos_id).unwrap();
    assert_position_liquidated(&closed).unwrap();
}
