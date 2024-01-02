use std::collections::HashSet;

use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::prelude::*;

// Tests that each position gets its own liquifunding cadence
#[test]
fn randomization() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market.exec_crank_till_finished(&trader).unwrap();

    let mut queue_resps = Vec::new();

    for _ in 0..50 {
        queue_resps.push(
            market
                .exec_open_position_queue_only(
                    &trader,
                    "5",
                    "5",
                    DirectionToBase::Long,
                    "2",
                    None,
                    None,
                    None,
                )
                .unwrap(),
        );
    }

    let mut timestamps = HashSet::new();

    for resp in queue_resps {
        // need to crank one at a time so that we don't accidentally open multiple positions in the same block
        let (pos_id, _) = market
            .exec_open_position_process_queue_response(&trader, resp, Some(1))
            .unwrap();
        let pos = market.query_position(pos_id).unwrap();
        let is_new = timestamps.insert(pos.next_liquifunding);

        assert!(
            is_new,
            "Duplicated next_liquifunding: {}",
            pos.next_liquifunding
        );
    }
}
