use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, time::TimeJump, PerpsApp};
use msg::prelude::*;

// not really a failable test
// more of a demo to show how we can
// control logging in the course of tests
#[test]
#[ignore]
fn logging_works() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // default is to not log the time jumps
    market.exec_crank(&cranker).unwrap();

    // jump forward to part of an epoch - and log it
    market.set_log_block_time_changes(true);
    market
        .set_time(TimeJump::FractionalLiquifundings(0.25))
        .unwrap();

    // jump forward an hour - without logging it
    market.set_log_block_time_changes(false);
    market.set_time(TimeJump::Hours(1)).unwrap();

    // jump forward one block - and log it
    market.set_log_block_time_changes(true);
    market.set_time(TimeJump::Blocks(1)).unwrap();

    // crank and log the time jump
    market.exec_refresh_price().unwrap(); // so we aren't in a stale state
    market.set_log_block_time_changes(true);
    market.exec_crank(&cranker).unwrap();

    // create a scenario so we get interesting crank info
    market.set_log_block_time_changes(false);
    let (_, _) = market
        .exec_open_position(
            &trader,
            "3",
            "3",
            DirectionToBase::Long,
            "3",
            None,
            None,
            None,
        )
        .unwrap();
    market.exec_refresh_price().unwrap();
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_refresh_price().unwrap();

    let res = market.exec_crank_till_finished(&cranker).unwrap();
    println!("{:#?}", res);
}
