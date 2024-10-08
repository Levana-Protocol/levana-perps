use std::collections::HashSet;

use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, time::TimeJump, PerpsApp};
use perpswap::prelude::DirectionToBase;

#[test]
fn event_collision() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    for _ in 0..3 {
        market
            .exec_open_position(
                &trader,
                "5",
                "5",
                DirectionToBase::Long,
                "2",
                None,
                None,
                None,
            )
            .unwrap();
    }

    market.set_time(TimeJump::Liquifundings(30)).unwrap();
    market.exec_refresh_price().unwrap();
    let res = market.exec_crank(&trader).unwrap();

    let iter = res.events.iter().map(|e| e.ty.clone());

    let mut event_types = HashSet::new();
    for event_type in iter {
        if event_types.contains(&event_type) {
            panic!("event type collision: {}", event_type);
        }

        event_types.insert(event_type);
    }
}
