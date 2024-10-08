use levana_perpswap_multi_test::{
    config::{SpotPriceKind, DEFAULT_MARKET},
    market_wrapper::PerpsMarket,
    time::TimeJump,
    PerpsApp,
};
use perpswap::prelude::*;

#[test]
fn oracle_open_position() {
    let market = PerpsMarket::new_with_type(
        PerpsApp::new_cell().unwrap(),
        DEFAULT_MARKET.collateral_type,
        true,
        SpotPriceKind::Oracle,
    )
    .unwrap();
    let trader = market.clone_trader(0).unwrap();

    let queue_res = market
        .exec_open_position_queue_only(
            &trader,
            "100",
            "9",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    // These steps are necessary
    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_refresh_price().unwrap();

    market
        .exec_open_position_process_queue_response(&trader, queue_res, None)
        .unwrap();
}
