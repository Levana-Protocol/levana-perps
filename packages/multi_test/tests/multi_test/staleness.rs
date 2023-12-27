use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, position_helpers::assert_position_liquidated, time::TimeJump,
    PerpsApp,
};
use msg::prelude::*;

#[test]
fn staleness() {
    panic!("This test needs to be reviewed and updated due to removing stale price");
    // let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    // return_unless_market_collateral_quote!(market);

    // let trader = market.clone_trader(0).unwrap();
    // let cranker = market.clone_trader(1).unwrap();

    // // Make sure there's a recent active price
    // market.exec_set_price("1".try_into().unwrap()).unwrap();

    // let status = market.query_status().unwrap();
    // assert_eq!(status.stale_liquifunding, None);
    // assert_eq!(status.stale_price, None);

    // let config = market.query_config().unwrap();
    // market
    //     .set_time(TimeJump::Seconds(
    //         config.price_update_too_old_seconds as i64 - 60,
    //     ))
    //     .unwrap();

    // let status = market.query_status().unwrap();
    // assert_eq!(status.stale_liquifunding, None);
    // assert_eq!(status.stale_price, None);

    // market.set_time(TimeJump::Seconds(62)).unwrap();
    // let StatusResp {
    //     stale_liquifunding,
    //     stale_price,
    //     ..
    // } = market.query_status().unwrap();

    // assert_eq!(stale_liquifunding, None);
    // assert_ne!(stale_price, None);

    // market.exec_set_price("10".try_into().unwrap()).unwrap();

    // let status = market.query_status().unwrap();
    // assert_eq!(status.stale_liquifunding, None);
    // assert_eq!(status.stale_price, None);

    // let (pos_id, _) = market
    //     .exec_open_position(
    //         &trader,
    //         "100",
    //         "10",
    //         DirectionToBase::Long,
    //         "10",
    //         None,
    //         None,
    //         None,
    //     )
    //     .unwrap();

    // market.exec_crank_till_finished(&cranker).unwrap();

    // let status = market.query_status().unwrap();
    // assert_eq!(status.stale_liquifunding, None);
    // assert_eq!(status.stale_price, None);

    // // Move ahead in time, do a price update, should be inactive
    // market
    //     .set_time(TimeJump::Seconds(
    //         (config.liquifunding_delay_seconds + config.staleness_seconds) as i64 * 4,
    //     ))
    //     .unwrap();

    // let StatusResp {
    //     stale_liquifunding,
    //     stale_price,
    //     ..
    // } = market.query_status().unwrap();
    // assert_ne!(stale_price, None);
    // assert_ne!(stale_liquifunding, None);

    // market.exec_set_price("1".try_into().unwrap()).unwrap();
    // let StatusResp {
    //     stale_liquifunding,
    //     stale_price,
    //     ..
    // } = market.query_status().unwrap();
    // assert_eq!(stale_price, None);
    // assert_ne!(stale_liquifunding, None);

    // market
    //     .exec_open_position(
    //         &trader,
    //         "100",
    //         "10",
    //         DirectionToBase::Long,
    //         "10",
    //         None,
    //         None,
    //         None,
    //     )
    //     .unwrap_err();
    // market
    //     .exec_update_position_leverage(&trader, pos_id, "1".try_into().unwrap(), None)
    //     .unwrap_err();

    // market
    //     .exec_close_position(&trader, pos_id, None)
    //     .unwrap_err();

    // market.exec_crank_till_finished(&trader).unwrap();

    // let (pos_id, _) = market
    //     .exec_open_position(
    //         &trader,
    //         "100",
    //         "10",
    //         DirectionToBase::Long,
    //         "10",
    //         None,
    //         None,
    //         None,
    //     )
    //     .unwrap();

    // market.query_position(pos_id).unwrap();

    // market
    //     .exec_update_position_leverage(&trader, pos_id, "8".try_into().unwrap(), None)
    //     .unwrap();

    // market.exec_close_position(&trader, pos_id, None).unwrap();
}

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
