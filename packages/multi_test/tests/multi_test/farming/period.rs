use msg::contracts::farming::entry::FarmingPeriodResp;

use crate::prelude::*;

#[test]
fn farming_period() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // TODO - get this from config
    const LOCKDROP_START_DURATION: Duration = Duration::from_seconds(60 * 60 * 24 * 12);
    const LOCKDROP_SUNSET_DURATION: Duration = Duration::from_seconds(60 * 60 * 24 * 2);

    let get_period = || market.query_farming_stats().period;

    let assert_unable_to_start_launch_or_lockdrop = || {
        // cannot launch without going through sunset and review first
        market.exec_farming_start_launch(None).unwrap_err();
        // cannot start lockdrop after started
        market.exec_farming_start_lockdrop(None).unwrap_err();
    };

    assert_eq!(
        get_period(),
        FarmingPeriodResp::Inactive {
            lockdrop_start: None
        }
    );

    // cannot launch without going through lockdrop first
    market.exec_farming_start_launch(None).unwrap_err();

    // schedule a lockdrop 12 hours from now
    let lockdrop_start = market.now() + Duration::from_seconds(60 * 60 * 24);
    market
        .exec_farming_start_lockdrop(Some(lockdrop_start))
        .unwrap();
    assert_eq!(
        get_period(),
        FarmingPeriodResp::Inactive {
            lockdrop_start: Some(lockdrop_start)
        }
    );
    // cannot launch without going through sunset and review first
    market.exec_farming_start_launch(None).unwrap_err();

    // reschedule to a day from now
    let lockdrop_start = market.now() + Duration::from_seconds(60 * 60 * 24);
    market
        .exec_farming_start_lockdrop(Some(lockdrop_start))
        .unwrap();
    assert_eq!(
        get_period(),
        FarmingPeriodResp::Inactive {
            lockdrop_start: Some(lockdrop_start)
        }
    );
    // cannot launch without going through sunset and review first
    market.exec_farming_start_launch(None).unwrap_err();

    // jump 8 hours, we're still scheduled
    market.set_time(TimeJump::Hours(8)).unwrap();
    assert_eq!(
        get_period(),
        FarmingPeriodResp::Inactive {
            lockdrop_start: Some(lockdrop_start)
        }
    );
    market.exec_farming_start_launch(None).unwrap_err();

    // jump the rest of the day, so we're in lockdrop mode
    market.set_time(TimeJump::Hours(16)).unwrap();
    let lockdrop_period = FarmingPeriodResp::Lockdrop {
        started_at: lockdrop_start,
        sunset_start: lockdrop_start + LOCKDROP_START_DURATION,
    };
    assert_eq!(get_period(), lockdrop_period);
    assert_unable_to_start_launch_or_lockdrop();

    // jump 3 days - we're still in lockdrop
    market.set_time(TimeJump::Hours(24 * 3)).unwrap();
    assert_eq!(get_period(), lockdrop_period);
    assert_unable_to_start_launch_or_lockdrop();

    // jump 10 days (13 days into the lockdrop) - now in sunset
    let sunset_period = FarmingPeriodResp::Sunset {
        started_at: lockdrop_start + LOCKDROP_START_DURATION,
        review_start: lockdrop_start + LOCKDROP_START_DURATION + LOCKDROP_SUNSET_DURATION,
    };
    market.set_time(TimeJump::Hours(24 * 10)).unwrap();
    assert_eq!(get_period(), sunset_period);
    assert_unable_to_start_launch_or_lockdrop();

    // jump half a day (13.5 days into the lockdrop) - still in sunset
    market.set_time(TimeJump::Hours(12)).unwrap();
    assert_eq!(get_period(), sunset_period);
    assert_unable_to_start_launch_or_lockdrop();

    // jump 4.5 days (18 days into the lockdrop) - now in review
    let review_period = FarmingPeriodResp::Review {
        started_at: Some(lockdrop_start + LOCKDROP_START_DURATION + LOCKDROP_SUNSET_DURATION),
        launch_start: None,
    };
    market.set_time(TimeJump::Hours((24 * 4) + 12)).unwrap();
    assert_eq!(get_period(), review_period);
    market.exec_farming_start_lockdrop(None).unwrap_err();

    // review waits for manual trigger, even 100 days later
    market.set_time(TimeJump::Hours(24 * 100)).unwrap();
    assert_eq!(get_period(), review_period);
    market.exec_farming_start_lockdrop(None).unwrap_err();

    // schedule a launch 10 days from now
    let launch_start = market.now() + Duration::from_seconds(60 * 60 * 24 * 10);
    let review_period = FarmingPeriodResp::Review {
        // we lose the info about when review started (who cares anyway, it can be indexed with events if we *really* need it)
        started_at: None,
        launch_start: Some(launch_start),
    };
    market
        .exec_farming_start_launch(Some(launch_start))
        .unwrap();
    assert_eq!(get_period(), review_period);
    market.exec_farming_start_lockdrop(None).unwrap_err();

    // actually, make it 3 days from now
    let launch_start = market.now() + Duration::from_seconds(60 * 60 * 24 * 3);
    let review_period = FarmingPeriodResp::Review {
        // we lose the info about when review started (who cares anyway, it can be indexed with events if we *really* need it)
        started_at: None,
        launch_start: Some(launch_start),
    };
    market
        .exec_farming_start_launch(Some(launch_start))
        .unwrap();
    assert_eq!(get_period(), review_period);
    market.exec_farming_start_lockdrop(None).unwrap_err();

    // A day later, still in review, pre-launch
    market.set_time(TimeJump::Hours(24)).unwrap();
    assert_eq!(get_period(), review_period);
    market.exec_farming_start_lockdrop(None).unwrap_err();

    // 2 days later - launched!
    let launch_period = FarmingPeriodResp::Launched {
        started_at: launch_start,
    };
    market.set_time(TimeJump::Hours(24 * 2)).unwrap();
    assert_eq!(get_period(), launch_period);

    // this is the end of the road
    assert_unable_to_start_launch_or_lockdrop();

    // even if we jump a bunch
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    assert_unable_to_start_launch_or_lockdrop();
    assert_eq!(get_period(), launch_period);
}
