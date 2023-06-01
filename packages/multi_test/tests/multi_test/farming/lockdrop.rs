use msg::contracts::farming::entry::defaults::lockdrop_month_seconds;
use msg::contracts::farming::entry::{defaults::lockdrop_buckets, FarmerLockdropStats};

use crate::prelude::*;

#[test]
fn farming_lockdrop_basic() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let farmer = market.clone_lp(0).unwrap();
    let buckets = lockdrop_buckets();

    // not allowed, currently in inactive period
    market
        .exec_farming_lockdrop_deposit(&farmer, "100".parse().unwrap(), buckets[0].bucket_id)
        .unwrap_err();

    // start the lockdrop
    market.exec_farming_start_lockdrop(None).unwrap();

    // can now deposit
    market
        .exec_farming_lockdrop_deposit(&farmer, "100".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();
    let farmer_stats = market.query_farming_farmer_stats(&farmer).unwrap();
    assert_eq!(farmer_stats.lockdrops.len(), 1);

    // withdraw all
    market
        .exec_farming_lockdrop_withdraw(&farmer, "100".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();
    let farmer_stats = market.query_farming_farmer_stats(&farmer).unwrap();
    assert_eq!(farmer_stats.lockdrops.len(), 0);

    // deposit again
    market
        .exec_farming_lockdrop_deposit(&farmer, "70".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();
    let farmer_stats = market.query_farming_farmer_stats(&farmer).unwrap();
    assert_eq!(farmer_stats.lockdrops.len(), 1);

    // withdraw and deposit more, during pre-sunset (bring total to 100)
    market
        .exec_farming_lockdrop_withdraw(&farmer, "40".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();
    market
        .exec_farming_lockdrop_deposit(&farmer, "70".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();
    let farmer_stats = market.query_farming_farmer_stats(&farmer).unwrap();
    let farmer_bucket_stats = &farmer_stats.lockdrops[0];
    assert_eq!(farmer_bucket_stats.total, "100".parse().unwrap());

    // move to sunset, cannot withdraw all
    market.set_time(TimeJump::Hours(24 * 13)).unwrap();
    market
        .exec_farming_lockdrop_withdraw(&farmer, "100".parse().unwrap(), buckets[0].bucket_id)
        .unwrap_err();

    // but can withdraw less than half
    market
        .exec_farming_lockdrop_withdraw(&farmer, "40".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();

    // can also deposit a bunch - though this won't effect our sunset withdrawal limits
    market
        .exec_farming_lockdrop_deposit(&farmer, "100".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();

    // see the new total
    let farmer_stats = market.query_farming_farmer_stats(&farmer).unwrap();
    let farmer_bucket_stats = &farmer_stats.lockdrops[0];
    assert_eq!(farmer_bucket_stats.total, "160".parse().unwrap());

    // cannot withdraw more than half of the *original* lockdrop (this would total 55 altogether, half is 50)
    market
        .exec_farming_lockdrop_withdraw(&farmer, "15".parse().unwrap(), buckets[0].bucket_id)
        .unwrap_err();

    // can still withdraw a bit more though
    market
        .exec_farming_lockdrop_withdraw(&farmer, "3".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();
    let farmer_stats = market.query_farming_farmer_stats(&farmer).unwrap();
    let farmer_bucket_stats = &farmer_stats.lockdrops[0];
    assert_eq!(farmer_bucket_stats.total, "157".parse().unwrap());

    // move waaay into review period
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();

    // cannot withdraw anything
    market
        .exec_farming_lockdrop_withdraw(&farmer, "1".parse().unwrap(), buckets[0].bucket_id)
        .unwrap_err();

    // launch
    market.exec_farming_start_launch(None).unwrap();

    // still cannot withdraw anything
    market
        .exec_farming_lockdrop_withdraw(&farmer, "1".parse().unwrap(), buckets[0].bucket_id)
        .unwrap_err();

    // when lockdrop expires, we can withdraw
    market
        .set_time(TimeJump::Seconds(86400 * buckets[0].bucket_id.0 as i64))
        .unwrap();
    market
        .exec_farming_lockdrop_withdraw(&farmer, "1".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();

    // stats are what we expect
    let farmer_bucket_stats = market
        .query_farming_farmer_stats(&farmer)
        .unwrap()
        .lockdrops
        .pop()
        .unwrap();

    assert_eq!(
        farmer_bucket_stats,
        FarmerLockdropStats {
            bucket_id: buckets[0].bucket_id,
            total: "156".parse().unwrap(),
            deposit_before_sunset: "140".parse().unwrap(),
            deposit_after_sunset: "100".parse().unwrap(),
            withdrawal_before_sunset: "40".parse().unwrap(),
            withdrawal_after_sunset: "43".parse().unwrap(),
            withdrawal_after_launch: "1".parse().unwrap(),
        }
    );

    // in fact we can withdraw a *lot*
    market
        .exec_farming_lockdrop_withdraw(&farmer, "150".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();

    // stats are what we expect
    let farmer_bucket_stats = market
        .query_farming_farmer_stats(&farmer)
        .unwrap()
        .lockdrops
        .pop()
        .unwrap();

    assert_eq!(
        farmer_bucket_stats,
        FarmerLockdropStats {
            bucket_id: buckets[0].bucket_id,
            total: "6".parse().unwrap(),
            deposit_before_sunset: "140".parse().unwrap(),
            deposit_after_sunset: "100".parse().unwrap(),
            withdrawal_before_sunset: "40".parse().unwrap(),
            withdrawal_after_sunset: "43".parse().unwrap(),
            withdrawal_after_launch: "151".parse().unwrap(),
        }
    );

    // in fact we can withdraw *everything*
    market
        .exec_farming_lockdrop_withdraw(&farmer, "6".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();

    // and we're no longer part of the lockdrop
    assert!(market
        .query_farming_farmer_stats(&farmer)
        .unwrap()
        .lockdrops
        .is_empty());
}

#[test]
fn test_query_lockdrop_rewards() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let buckets = lockdrop_buckets();
    let farmers = [
        market.clone_lp(0).unwrap(),
        market.clone_lp(1).unwrap(),
        market.clone_lp(2).unwrap(),
    ];
    let token = market.setup_lvn_rewards("10000");

    market.automatic_time_jump_enabled = false;
    market.exec_farming_start_lockdrop(None).unwrap();

    // Farmers deposit into the first three buckets respectively
    for (i, addr) in farmers.iter().enumerate() {
        market
            .exec_farming_lockdrop_deposit(addr, "100".parse().unwrap(), buckets[i].bucket_id)
            .unwrap();
    }

    // Farmer1 makes an additional deposit into the last bucket
    market
        .exec_farming_lockdrop_deposit(
            &farmers[0],
            "200".parse().unwrap(),
            buckets[buckets.len() - 1].bucket_id,
        )
        .unwrap();

    // Jump to review period
    market
        .set_time(TimeJump::Seconds(60 * 60 * 24 * 14))
        .unwrap();
    market
        .exec_farming_set_lockdrop_rewards("384".parse().unwrap(), &token)
        .unwrap();
    market.exec_farming_start_launch(None).unwrap();

    // Jump a quarter way to the end of lockup period

    let unlock_duration: i64 = lockdrop_month_seconds().into();
    market
        .set_time(TimeJump::Seconds(unlock_duration / 4))
        .unwrap();

    let stats0 = market.query_farming_farmer_stats(&farmers[0]).unwrap();
    let stats1 = market.query_farming_farmer_stats(&farmers[1]).unwrap();
    let stats2 = market.query_farming_farmer_stats(&farmers[2]).unwrap();

    assert_eq!(stats0.lockdrops.len(), 2);
    assert_eq!(stats0.lockdrops[0].total, "100".parse().unwrap());
    assert_eq!(stats0.lockdrops[1].total, "200".parse().unwrap());
    assert_eq!(stats0.lockdrop_available, "76".parse().unwrap());
    assert_eq!(stats0.lockdrop_locked, "228".parse().unwrap());

    assert_eq!(stats1.lockdrops.len(), 1);
    assert_eq!(stats1.lockdrops[0].total, "100".parse().unwrap());
    assert_eq!(stats1.lockdrop_available, "7".parse().unwrap());
    assert_eq!(stats1.lockdrop_locked, "21".parse().unwrap());

    assert_eq!(stats2.lockdrops.len(), 1);
    assert_eq!(stats2.lockdrops[0].total, "100".parse().unwrap());
    assert_eq!(stats2.lockdrop_available, "13".parse().unwrap());
    assert_eq!(stats2.lockdrop_locked, "39".parse().unwrap());

    let assert_total = || {
        let stats0 = market.query_farming_farmer_stats(&farmers[0]).unwrap();
        let stats1 = market.query_farming_farmer_stats(&farmers[1]).unwrap();
        let stats2 = market.query_farming_farmer_stats(&farmers[2]).unwrap();

        assert_eq!(stats0.lockdrop_available, "304".parse().unwrap());
        assert_eq!(stats1.lockdrop_available, "28".parse().unwrap());
        assert_eq!(stats2.lockdrop_available, "52".parse().unwrap());
    };

    // Jump to the end of the lockup period

    market
        .set_time(TimeJump::Seconds(unlock_duration / 4 * 3))
        .unwrap();
    assert_total();

    // Jump passed the end of the lockup period

    market.set_time(TimeJump::Hours(3)).unwrap();
    assert_total();
}

#[test]
fn test_claim_lockdrop_rewards() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let buckets = lockdrop_buckets();
    let farmers = [
        market.clone_lp(0).unwrap(),
        market.clone_lp(1).unwrap(),
        market.clone_lp(2).unwrap(),
    ];
    let token = market.setup_lvn_rewards("1000000");

    market.automatic_time_jump_enabled = false;
    market.exec_farming_start_lockdrop(None).unwrap();

    // Farmers deposit into the first three buckets respectively
    for (i, addr) in farmers.iter().enumerate() {
        market
            .exec_farming_lockdrop_deposit(addr, "100".parse().unwrap(), buckets[i].bucket_id)
            .unwrap();
    }

    // Jump to review period
    market
        .set_time(TimeJump::Seconds(60 * 60 * 24 * 14))
        .unwrap();
    market
        .exec_farming_set_lockdrop_rewards("90".parse().unwrap(), &token)
        .unwrap();
    market.exec_farming_start_launch(None).unwrap();

    // Jump a quarter way to the end of lockup period

    let unlock_duration: i64 = lockdrop_month_seconds().into();
    market
        .set_time(TimeJump::Seconds(unlock_duration / 2))
        .unwrap();

    // Claim and assert

    for addr in &farmers {
        market.exec_farming_claim_lockdrop_rewards(addr).unwrap();
    }

    let balance0 = market.query_reward_token_balance(&token, &farmers[0]);
    let balance1 = market.query_reward_token_balance(&token, &farmers[1]);
    let balance2 = market.query_reward_token_balance(&token, &farmers[2]);

    assert_eq!(balance0, "5".parse().unwrap());
    assert_eq!(balance1, "14".parse().unwrap());
    assert_eq!(balance2, "26".parse().unwrap());

    let assert_total = || {
        for addr in &farmers {
            market.exec_farming_claim_lockdrop_rewards(addr).unwrap();
        }

        let balance0 = market.query_reward_token_balance(&token, &farmers[0]);
        let balance1 = market.query_reward_token_balance(&token, &farmers[1]);
        let balance2 = market.query_reward_token_balance(&token, &farmers[2]);

        assert_eq!(balance0, "10".parse().unwrap());
        assert_eq!(balance1, "28".parse().unwrap());
        assert_eq!(balance2, "52".parse().unwrap());
    };

    // Jump to the end of the lockup period

    market
        .set_time(TimeJump::Seconds(unlock_duration / 4 * 3))
        .unwrap();
    assert_total();

    // Jump passed the end of the lockup period

    market.set_time(TimeJump::Hours(3)).unwrap();
    assert_total();
}
