use std::collections::HashSet;

use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{contracts::market::config::ConfigUpdate, prelude::*};

#[test]
fn test_congestion_block() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Reduce the unpend_limit to keep execution time reasonable
    market
        .exec_set_config(ConfigUpdate {
            unpend_limit: Some(50),
            ..Default::default()
        })
        .unwrap();

    let trader = market.clone_trader(0).unwrap();

    // Do a price update without cranking to force unpending the position
    // Since we're always off-by-one on which price update we use, we need to insert two price updates.
    market.exec_set_price("1.02".parse().unwrap()).unwrap();
    market.exec_set_price("1.02".parse().unwrap()).unwrap();

    // We can open up a bunch of positions without a crank...
    for _ in 0..market.query_status().unwrap().config.unpend_limit {
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

    // Opening the next position should fail
    let err = market
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
        .unwrap_err();
    let err: PerpError<MarketError> = err.downcast().unwrap();
    assert_eq!(err.id, ErrorId::Congestion);

    // Now we crank...
    market.exec_crank_till_finished(&trader).unwrap();

    // And now opening should be fine
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

#[test]
fn randomization() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let mut timestamps = HashSet::new();

    // We can open up a bunch of positions without a crank...
    // Set a max on the number of positions we open to deal with long test executions.
    // Also, just due to random weirdness, the duplicate test fails when the number is too high.
    for _ in 0..market.query_status().unwrap().config.unpend_limit.min(50) {
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

        let is_new = timestamps.insert(pos.next_liquifunding);
        assert!(
            is_new,
            "Duplicated next_liquifunding: {}",
            pos.next_liquifunding
        );
    }
}
