use msg::contracts::farming::entry::FarmingPeriod;

use crate::prelude::*;

#[test]
fn farming_period() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let get_period = || market.query_farming_stats().period;
    let get_schedule_countdown = || market.query_farming_stats().schedule_countdown;

    let assert_unable_to_start_launch_or_lockdrop = || {
        // cannot launch without going through sunset and review first
        market.exec_farming_start_launch(None).unwrap_err();
        // cannot start lockdrop after started
        market.exec_farming_start_lockdrop(None).unwrap_err();
    };

    assert_eq!(get_period(), FarmingPeriod::Inactive);
    assert_eq!(get_schedule_countdown(), None);

    // cannot launch without going through lockdrop first
    market.exec_farming_start_launch(None).unwrap_err();

    // schedule a lockdrop one day from now
    market
        .exec_farming_start_lockdrop(Some(market.now() + Duration::from_seconds(60 * 60 * 24)))
        .unwrap();
    assert_eq!(get_period(), FarmingPeriod::LockdropScheduled);
    assert_unable_to_start_launch_or_lockdrop();

    // check the countdown, but account for the 7 second block time at the time of scheduling
    assert_eq!(
        get_schedule_countdown(),
        Some(Duration::from_seconds((60 * 60 * 24) - 7))
    );

    // jump 8 hours, we're still scheduled
    market.set_time(TimeJump::Hours(8)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::LockdropScheduled);
    assert_unable_to_start_launch_or_lockdrop();
    assert_eq!(
        get_schedule_countdown(),
        Some(Duration::from_seconds((60 * 60 * 16) - 7))
    );

    // jump the rest of the day, we're now in lockdrop proper
    market.set_time(TimeJump::Hours(16)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::Lockdrop);
    assert_unable_to_start_launch_or_lockdrop();
    assert_eq!(get_schedule_countdown(), None);

    // jump 3 days - we're still in lockdrop
    market.set_time(TimeJump::Hours(24 * 3)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::Lockdrop);
    assert_unable_to_start_launch_or_lockdrop();
    assert_eq!(get_schedule_countdown(), None);

    // jump 10 days (13 days into the lockdrop) - now in sunset
    market.set_time(TimeJump::Hours(24 * 10)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::Sunset);
    assert_unable_to_start_launch_or_lockdrop();
    assert_eq!(get_schedule_countdown(), None);

    // jump 5 days (18 days into the lockdrop) - now in review
    market.set_time(TimeJump::Hours(24 * 5)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::Review);
    assert_eq!(get_schedule_countdown(), None);

    // cannot start lockdrop after started
    market.exec_farming_start_lockdrop(None).unwrap_err();

    // schedule a launch one day from now
    market
        .exec_farming_start_launch(Some(market.now() + Duration::from_seconds(60 * 60 * 24)))
        .unwrap();
    assert_eq!(get_period(), FarmingPeriod::LaunchScheduled);
    assert_unable_to_start_launch_or_lockdrop();
    // check the countdown, but account for the 7 second block time at the time of scheduling
    assert_eq!(
        get_schedule_countdown(),
        Some(Duration::from_seconds((60 * 60 * 24) - 7))
    );

    // jump 8 hours , we're still scheduled
    market.set_time(TimeJump::Hours(8)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::LaunchScheduled);
    assert_unable_to_start_launch_or_lockdrop();
    assert_eq!(
        get_schedule_countdown(),
        Some(Duration::from_seconds((60 * 60 * 16) - 7))
    );

    // jump the rest of the day, we're now in launch proper
    market.set_time(TimeJump::Hours(16)).unwrap();
    assert_eq!(get_period(), FarmingPeriod::Launched);
    assert_eq!(get_schedule_countdown(), None);

    // this is the end of the road
    assert_unable_to_start_launch_or_lockdrop();

    // even if we jump a bunch
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    assert_unable_to_start_launch_or_lockdrop();
    assert_eq!(get_period(), FarmingPeriod::Launched);
    assert_eq!(get_schedule_countdown(), None);
}
