use std::collections::HashSet;

use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::prelude::*;

#[test]
fn randomization() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let mut timestamps = HashSet::new();

    market.exec_crank_till_finished(&trader).unwrap();

    // Each position gets its own liquifunding cadence 
    for _ in 0..50 {
        let (pos_id, _) = market
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
        let pos = market.query_position(pos_id).unwrap();

        println!("{}: {}", pos_id, pos.next_liquifunding);

        let is_new = timestamps.insert(pos.next_liquifunding);
        assert!(
            is_new,
            "Duplicated next_liquifunding: {}",
            pos.next_liquifunding
        );
    }
}
