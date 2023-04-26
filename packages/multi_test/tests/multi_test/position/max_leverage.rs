use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::prelude::*;

#[test]
fn max_leverage_long() {
    max_leverage_helper(DirectionToBase::Long);
}

#[test]
fn max_leverage_short() {
    max_leverage_helper(DirectionToBase::Short);
}

fn max_leverage_helper(direction: DirectionToBase) {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let config = market.query_config().unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            config.max_leverage.to_string().as_str(),
            direction,
            "2.0",
            None,
            None,
            None,
        )
        .unwrap();

    // PERP-799 try to update the position
    market
        .exec_update_position_max_gains(&trader, pos_id, "2.1".parse().unwrap())
        .unwrap();
}
