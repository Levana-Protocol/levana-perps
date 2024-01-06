use cosmwasm_std::Uint128;
use levana_perpswap_multi_test::config::{DEFAULT_MARKET, TEST_CONFIG};
use levana_perpswap_multi_test::market_wrapper::PerpsMarket;
use levana_perpswap_multi_test::time::TimeJump;
use levana_perpswap_multi_test::PerpsApp;
use msg::contracts::cw20::entry::{QueryMsg as Cw20QueryMsg, TokenInfoResponse};
use msg::contracts::farming::entry::defaults::lockdrop_month_seconds;
use msg::contracts::farming::entry::{
    defaults::lockdrop_buckets, FarmerLockdropStats, LockdropBucketConfig,
};
use msg::contracts::liquidity_token::LiquidityTokenKind;
use msg::prelude::*;

fn setup_lockdrop() -> (PerpsMarket, Vec<LockdropBucketConfig>, [Addr; 3]) {
    let mut market = PerpsMarket::new_with_type(
        PerpsApp::new_cell().unwrap(),
        DEFAULT_MARKET.collateral_type,
        false,
        DEFAULT_MARKET.spot_price,
    )
    .unwrap();
    let buckets = lockdrop_buckets();
    let farmers = [
        market.clone_lp(0).unwrap(),
        market.clone_lp(1).unwrap(),
        market.clone_lp(2).unwrap(),
    ];

    market.automatic_time_jump_enabled = false;
    market.exec_farming_start_lockdrop(None).unwrap();

    // Farmers deposit into the first three buckets respectively
    for (i, addr) in farmers.iter().enumerate() {
        market
            .exec_farming_lockdrop_deposit(addr, "100".parse().unwrap(), buckets[i].bucket_id)
            .unwrap();
    }

    (market, buckets, farmers)
}

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

    // assert totals before launch
    let farming_stats = market.query_farming_status();
    assert_eq!(farming_stats.xlp, LpToken::zero());
    assert_eq!(farming_stats.farming_tokens, "157".parse().unwrap());

    // launch
    market.exec_farming_start_launch().unwrap();

    // assert totals after launch
    let farming_stats = market.query_farming_status();
    assert_eq!(farming_stats.xlp, "157".parse().unwrap());
    assert_eq!(farming_stats.farming_tokens, "157".parse().unwrap());

    // Assert we cannot withdraw anything after launch
    market
        .exec_farming_lockdrop_withdraw(&farmer, "1".parse().unwrap(), buckets[0].bucket_id)
        .unwrap_err();

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
            total: "157".parse().unwrap(),
            deposit_before_sunset: "140".parse().unwrap(),
            deposit_after_sunset: "100".parse().unwrap(),
            withdrawal_before_sunset: "40".parse().unwrap(),
            withdrawal_after_sunset: "43".parse().unwrap(),
        }
    );
}

#[test]
fn test_query_lockdrop_rewards() {
    let (market, buckets, farmers) = setup_lockdrop();
    let token = market.mint_lvn_rewards("1000000", None);

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
    market.exec_farming_start_launch().unwrap();

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
    assert_eq!(stats0.lockdrop_rewards_available, "76".parse().unwrap());
    assert_eq!(stats0.lockdrop_rewards_locked, "228".parse().unwrap());

    assert_eq!(stats1.lockdrops.len(), 1);
    assert_eq!(stats1.lockdrops[0].total, "100".parse().unwrap());
    assert_eq!(stats1.lockdrop_rewards_available, "7".parse().unwrap());
    assert_eq!(stats1.lockdrop_rewards_locked, "21".parse().unwrap());

    assert_eq!(stats2.lockdrops.len(), 1);
    assert_eq!(stats2.lockdrops[0].total, "100".parse().unwrap());
    assert_eq!(stats2.lockdrop_rewards_available, "13".parse().unwrap());
    assert_eq!(stats2.lockdrop_rewards_locked, "39".parse().unwrap());

    let assert_total = || {
        let stats0 = market.query_farming_farmer_stats(&farmers[0]).unwrap();
        let stats1 = market.query_farming_farmer_stats(&farmers[1]).unwrap();
        let stats2 = market.query_farming_farmer_stats(&farmers[2]).unwrap();

        assert_eq!(stats0.lockdrop_rewards_available, "304".parse().unwrap());
        assert_eq!(stats1.lockdrop_rewards_available, "28".parse().unwrap());
        assert_eq!(stats2.lockdrop_rewards_available, "52".parse().unwrap());
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
    let (market, _buckets, farmers) = setup_lockdrop();
    let token = market.mint_lvn_rewards("1000000", None);

    // Jump to review period
    market
        .set_time(TimeJump::Seconds(60 * 60 * 24 * 14))
        .unwrap();
    market
        .exec_farming_set_lockdrop_rewards("90".parse().unwrap(), &token)
        .unwrap();
    market.exec_farming_start_launch().unwrap();

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

#[test]
fn test_lockdrop_locked_tokens() {
    let (market, buckets, farmers) = setup_lockdrop();
    let cw20_info: TokenInfoResponse = market
        .query_liquidity_token(LiquidityTokenKind::Xlp, &Cw20QueryMsg::TokenInfo {})
        .unwrap();
    let decimals = cw20_info.decimals as u32;

    // Farmer1 makes a additional deposits into second and third buckets
    market
        .exec_farming_lockdrop_deposit(&farmers[0], "200".parse().unwrap(), buckets[1].bucket_id)
        .unwrap();
    market
        .exec_farming_lockdrop_deposit(&farmers[0], "200".parse().unwrap(), buckets[2].bucket_id)
        .unwrap();

    // Jump to review period, transfer collateral, and launch the lockdrop

    market
        .set_time(TimeJump::Seconds(60 * 60 * 24 * 14))
        .unwrap();
    market.exec_farming_start_launch().unwrap();

    // Assert when no time has passed

    for farmer in &farmers {
        market.exec_farming_withdraw_xlp(farmer, None).unwrap_err();
        let xlp_balance = market
            .query_liquidity_token_balance_raw(LiquidityTokenKind::Xlp, farmer)
            .unwrap();
        assert_eq!(xlp_balance, Uint128::zero());
    }

    // Jump 1.5 months at a time while asserting withdrawals

    let jump = || {
        let one_and_a_half = (lockdrop_month_seconds() + lockdrop_month_seconds() / 2).into();
        market.set_time(TimeJump::Seconds(one_and_a_half)).unwrap();
    };
    let assert_balance = |farmer: &Addr, expected_delta: u64, expected_total: u64| {
        let expected_delta = Number::from(expected_delta);
        let expected_total = Number::from(expected_total);
        let raw_balance_before = market
            .query_liquidity_token_balance_raw(LiquidityTokenKind::Xlp, farmer)
            .unwrap();

        if expected_delta.is_zero() {
            market.exec_farming_withdraw_xlp(farmer, None).unwrap_err();
        } else {
            market.exec_farming_withdraw_xlp(farmer, None).unwrap();
        }

        let raw_balance_after = market
            .query_liquidity_token_balance_raw(LiquidityTokenKind::Xlp, farmer)
            .unwrap();
        let balance_before = Number::from_fixed_u128(raw_balance_before.into(), decimals);
        let balance_after = Number::from_fixed_u128(raw_balance_after.into(), decimals);
        let delta = balance_after.checked_sub(balance_before).unwrap();

        assert_eq!(delta, expected_delta);
        assert_eq!(balance_after, expected_total);
    };
    let (f0, f1, f2) = (&farmers[0], &farmers[1], &farmers[2]);

    // 1.5
    jump();

    for farmer in &farmers {
        assert_balance(farmer, 0, 0);
    }

    // 3
    jump();

    assert_balance(f0, 100, 100);
    assert_balance(f1, 0, 0);
    assert_balance(f2, 0, 0);

    // 4.5
    jump();

    assert_balance(f0, 0, 100);
    assert_balance(f1, 0, 0);
    assert_balance(f2, 0, 0);

    // 6
    jump();

    assert_balance(f0, 200, 300);
    assert_balance(f1, 100, 100);
    assert_balance(f2, 0, 0);

    // 7.5
    jump();

    assert_balance(f0, 0, 300);
    assert_balance(f1, 0, 100);
    assert_balance(f2, 0, 0);

    // 9
    jump();

    assert_balance(f0, 200, 500);
    assert_balance(f1, 0, 100);
    assert_balance(f2, 100, 100);

    // 10.5
    jump();

    assert_balance(f0, 0, 500);
    assert_balance(f1, 0, 100);
    assert_balance(f2, 0, 100);
}

#[test]
fn test_reinvest_yield() {
    // Setup

    let (mut market, _buckets, farmers) = setup_lockdrop();
    let trader0 = market.clone_trader(0).unwrap();

    // Jump to review period, transfer collateral, and launch the lockdrop

    market
        .set_time(TimeJump::Seconds(60 * 60 * 24 * 14))
        .unwrap();
    market.exec_farming_start_launch().unwrap();

    market.exec_refresh_price().unwrap();
    market.set_time(TimeJump::Seconds(60)).unwrap();
    market
        .exec_crank_till_finished(&Addr::unchecked("cranker"))
        .unwrap();

    // Open a position to accrue trading fees

    let farming_status_before = market.query_farming_status();

    market.automatic_time_jump_enabled = true;
    market
        .exec_open_position(
            &trader0,
            "100",
            "9",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();
    market.automatic_time_jump_enabled = false;

    // Reinvest and assert

    market.exec_farming_reinvest().unwrap();

    let farming_status_after = market.query_farming_status();
    assert!(farming_status_before.xlp < farming_status_after.xlp);
    assert!(farming_status_before.bonus < farming_status_after.bonus);

    // Test transfer bonus

    let owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
    let balance_before = market.query_collateral_balance(&owner).unwrap();
    market.exec_farming_transfer_bonus().unwrap();
    let balance_after = market.query_collateral_balance(&owner).unwrap();

    assert_eq!(
        balance_after,
        balance_before
            .checked_add(farming_status_after.bonus.into_number())
            .unwrap()
    );

    let farming_status = market.query_farming_status();
    assert_eq!(farming_status.bonus, Collateral::zero());

    // Withdraw xLP

    market
        .set_time(TimeJump::Seconds((lockdrop_month_seconds() * 4).into()))
        .unwrap();
    let balance_before = market
        .query_liquidity_token_balance_raw(LiquidityTokenKind::Xlp, &farmers[0])
        .unwrap();
    market.exec_farming_withdraw_xlp(&farmers[0], None).unwrap();
    let balance_after = market
        .query_liquidity_token_balance_raw(LiquidityTokenKind::Xlp, &farmers[0])
        .unwrap();

    assert_eq!(balance_before, Uint128::zero());
    assert!(
        LpToken::from_u128(balance_after.u128()).unwrap() > LpToken::from_u128(100u128).unwrap()
    );
}
