use msg::contracts::farming::entry::FarmingPeriod;

use crate::prelude::*;

#[test]
fn farming_period() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let get_period = || market.query_farming_stats().period;

    let assert_unable_to_start_launch_or_lockdrop = || {
        // cannot launch without going through sunset and review first
        market.exec_farming_start_launch().unwrap_err();
        // cannot start lockdrop after started
        market.exec_farming_start_lockdrop().unwrap_err();
    };

    assert_eq!(get_period(), FarmingPeriod::Inactive);

    // cannot launch without going through lockdrop first
    market.exec_farming_start_launch().unwrap_err();

    // start lockdrop
    market.exec_farming_start_lockdrop().unwrap();
    assert_eq!(get_period(), FarmingPeriod::Lockdrop);
    assert_unable_to_start_launch_or_lockdrop();

    // jump 3 days - we're still in lockdrop
    market.set_time(TimeJump::Hours(24 * 3)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::Lockdrop);
    assert_unable_to_start_launch_or_lockdrop();

    // jump 10 days (13 days into the lockdrop) - now in sunset
    market.set_time(TimeJump::Hours(24 * 10)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::Sunset);
    assert_unable_to_start_launch_or_lockdrop();

    // jump 5 days (18 days into the lockdrop) - now in review
    market.set_time(TimeJump::Hours(24 * 5)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::Review);

    // cannot start lockdrop after started
    market.exec_farming_start_lockdrop().unwrap_err();
    // but now we can launch
    market.exec_farming_start_launch().unwrap();

    assert_eq!(get_period(), FarmingPeriod::Launched);

    // this is the end of the road
    assert_unable_to_start_launch_or_lockdrop();

    // even if we jump a bunch
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    assert_unable_to_start_launch_or_lockdrop();
    assert_eq!(get_period(), FarmingPeriod::Launched);
}
