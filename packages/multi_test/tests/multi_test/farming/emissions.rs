use levana_perpswap_multi_test::market_wrapper::PerpsMarket;
use levana_perpswap_multi_test::time::TimeJump;
use levana_perpswap_multi_test::PerpsApp;
use levana_perpswap_multi_test::{
    arbitrary::farming::emissions::data::FarmingEmissions, config::TEST_CONFIG,
};
use msg::contracts::farming::entry::defaults::{
    bonus_ratio, lockdrop_buckets, lockdrop_month_seconds,
};
use msg::contracts::farming::entry::{
    Emissions, ExecuteMsg, FarmingPeriodResp, LockdropBucketStats, OwnerExecuteMsg,
};
use msg::contracts::farming::events::DepositSource;
use msg::prelude::*;
use msg::token::Token;
use proptest::prelude::*;

const EMISSIONS_DURATION: u32 = 20;
const EMISSIONS_REWARDS: &str = "200";

fn farming_deposit(market: &PerpsMarket, lp: &Addr) -> Result<()> {
    farming_deposit_from_source(market, lp, DepositSource::Xlp)
}

fn farming_withdraw(market: &PerpsMarket, lp: &Addr, amount: Option<&str>) -> Result<()> {
    market.exec_farming_withdraw_xlp(lp, amount.map(|s| s.parse().unwrap()))?;
    Ok(())
}

fn farming_deposit_from_source(
    market: &PerpsMarket,
    lp: &Addr,
    source: DepositSource,
) -> Result<()> {
    match source {
        DepositSource::Collateral => {
            market
                .exec_farming_deposit_collateral(lp, "100".parse().unwrap())
                .unwrap();
        }
        DepositSource::Lp => {
            market.exec_mint_and_deposit_liquidity(lp, "100".parse().unwrap())?;
            market.exec_farming_deposit_lp(lp, "100".parse().unwrap())?;
        }
        DepositSource::Xlp => {
            market.exec_mint_and_deposit_liquidity(lp, "100".parse().unwrap())?;
            market.exec_stake_lp(lp, Some("100".parse().unwrap()))?;
            market.exec_farming_deposit_xlp(lp, "100".parse().unwrap())?;
        }
    }

    Ok(())
}

fn move_past_lockdrop(market: &PerpsMarket) {
    market.exec_farming_start_lockdrop(None).unwrap();
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    market.exec_farming_start_launch().unwrap();
}

fn start_emissions(market: &PerpsMarket) -> Result<Token> {
    let token = market.mint_lvn_rewards(EMISSIONS_REWARDS, None);
    market.exec_farming_set_emissions(
        market.now(),
        EMISSIONS_DURATION,
        EMISSIONS_REWARDS.parse().unwrap(),
        token.clone(),
    )?;

    Ok(token)
}

#[test]
fn test_emissions() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);
    farming_deposit(&market, &lp).unwrap();
    start_emissions(&market).unwrap();

    // Test query farming rewards

    market.set_time(TimeJump::Seconds(5)).unwrap();
    let stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(stats.emission_rewards, "50".parse().unwrap());

    market.set_time(TimeJump::Seconds(15)).unwrap();
    let stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(stats.emission_rewards, "200".parse().unwrap());
}

#[test]
fn test_emissions_multiple_lps() {
    struct Lp<'a> {
        addr: Addr,
        amount: &'a str,
    }

    // market Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    market.automatic_time_jump_enabled = false;

    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();
    let lp2 = market.clone_lp(2).unwrap();

    let lps = [
        Lp {
            addr: lp0,
            amount: "100",
        },
        Lp {
            addr: lp1,
            amount: "100",
        },
        Lp {
            addr: lp2,
            amount: "200",
        },
    ];

    for lp in &lps {
        market
            .exec_mint_and_deposit_liquidity(&lp.addr, lp.amount.parse().unwrap())
            .unwrap();
        market.exec_stake_lp(&lp.addr, None).unwrap();
    }

    // Farming setup & deposit

    move_past_lockdrop(&market);
    start_emissions(&market).unwrap();

    // lp0
    market
        .exec_farming_deposit_xlp(&lps[0].addr, lps[0].amount.parse().unwrap())
        .unwrap();

    // lp1
    market.set_time(TimeJump::Seconds(5)).unwrap();
    market
        .exec_farming_deposit_xlp(
            &lps[1].addr,
            NonZero::new(lps[1].amount.parse().unwrap()).unwrap(),
        )
        .unwrap();

    // lp2
    market.set_time(TimeJump::Seconds(5)).unwrap();
    market
        .exec_farming_deposit_xlp(
            &lps[2].addr,
            NonZero::new(lps[2].amount.parse().unwrap()).unwrap(),
        )
        .unwrap();

    // Test halfway through emissions

    let lp0_stats = market.query_farming_farmer_stats(&lps[0].addr).unwrap();
    let lp1_stats = market.query_farming_farmer_stats(&lps[1].addr).unwrap();
    let lp2_stats = market.query_farming_farmer_stats(&lps[2].addr).unwrap();

    assert_eq!(lp0_stats.emission_rewards, "75".parse().unwrap());
    assert_eq!(lp1_stats.emission_rewards, "25".parse().unwrap());
    assert_eq!(lp2_stats.emission_rewards, "0".parse().unwrap());

    // Test after emissions complete

    market.set_time(TimeJump::Seconds(10)).unwrap();

    let lp0_stats = market.query_farming_farmer_stats(&lps[0].addr).unwrap();
    let lp1_stats = market.query_farming_farmer_stats(&lps[1].addr).unwrap();
    let lp2_stats = market.query_farming_farmer_stats(&lps[2].addr).unwrap();

    assert_eq!(lp0_stats.emission_rewards, "100".parse().unwrap());
    assert_eq!(lp1_stats.emission_rewards, "50".parse().unwrap());
    assert_eq!(lp2_stats.emission_rewards, "50".parse().unwrap());
}

#[test]
fn test_emission_bounds() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);

    // lp0 deposits before start of emissions
    farming_deposit(&market, &lp0).unwrap();

    market.set_time(TimeJump::Seconds(30)).unwrap();

    // lp1 deposits at start of emissions
    farming_deposit(&market, &lp1).unwrap();

    start_emissions(&market).unwrap();
    market
        .set_time(TimeJump::Seconds(EMISSIONS_DURATION.into()))
        .unwrap();

    // lp0 deposits at end of emissions
    farming_deposit(&market, &lp0).unwrap();

    // lp1 deposits after end of emissions
    farming_deposit(&market, &lp1).unwrap();

    let lp0_stats = market.query_farming_farmer_stats(&lp0).unwrap();
    assert_eq!(lp0_stats.emission_rewards, "100".parse().unwrap());

    let lp1_stats = market.query_farming_farmer_stats(&lp1).unwrap();
    assert_eq!(lp1_stats.emission_rewards, "100".parse().unwrap());
}

#[test]
fn test_multiple_emissions() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();
    let lp2 = market.clone_lp(2).unwrap();
    let lp3 = market.clone_lp(3).unwrap();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);

    // Execute first emissions with two LPs

    farming_deposit(&market, &lp0).unwrap();
    farming_deposit(&market, &lp1).unwrap();

    start_emissions(&market).unwrap();
    market.set_time(TimeJump::Seconds(100)).unwrap();

    let lp0_stats = market.query_farming_farmer_stats(&lp0).unwrap();
    assert_eq!(lp0_stats.emission_rewards, "100".parse().unwrap());

    let lp1_stats = market.query_farming_farmer_stats(&lp1).unwrap();
    assert_eq!(lp1_stats.emission_rewards, "100".parse().unwrap());

    // Execute second emissions with an additional two LPs

    start_emissions(&market).unwrap();
    farming_deposit(&market, &lp2).unwrap();

    // lp3 deposits 3/4 of the way in
    market
        .set_time(TimeJump::Seconds((EMISSIONS_DURATION * 3 / 4).into()))
        .unwrap();
    farming_deposit(&market, &lp3).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();

    let lp0_stats = market.query_farming_farmer_stats(&lp0).unwrap();
    assert_eq!(lp0_stats.emission_rewards, "162.5".parse().unwrap());

    let lp1_stats = market.query_farming_farmer_stats(&lp1).unwrap();
    assert_eq!(lp1_stats.emission_rewards, "162.5".parse().unwrap());

    let lp2_stats = market.query_farming_farmer_stats(&lp2).unwrap();
    assert_eq!(lp2_stats.emission_rewards, "62.5".parse().unwrap());

    let lp3_stats = market.query_farming_farmer_stats(&lp3).unwrap();
    assert_eq!(lp3_stats.emission_rewards, "12.5".parse().unwrap());
}

#[test]
fn test_deposit_collateral() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);

    // Deposit & assert

    let farming_stats_before = market.query_farming_status();
    farming_deposit_from_source(&market, &lp, DepositSource::Collateral).unwrap();

    let farmer_stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(farmer_stats.farming_tokens, "100".parse().unwrap());

    let farming_stats_after = market.query_farming_status();
    assert_eq!(
        farming_stats_after.xlp,
        farming_stats_before
            .xlp
            .checked_add("100".parse().unwrap())
            .unwrap()
    );
    assert_eq!(
        farming_stats_after.farming_tokens,
        farming_stats_before
            .farming_tokens
            .checked_add("100".parse().unwrap())
            .unwrap()
    );
}

#[test]
fn test_deposit_lp() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);

    // Deposit & assert

    let farming_stats_before = market.query_farming_status();
    farming_deposit_from_source(&market, &lp, DepositSource::Lp).unwrap();

    let farmer_stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(farmer_stats.farming_tokens, "100".parse().unwrap());

    let farming_stats_after = market.query_farming_status();
    assert_eq!(
        farming_stats_after.xlp,
        farming_stats_before
            .xlp
            .checked_add("100".parse().unwrap())
            .unwrap()
    );
    assert_eq!(
        farming_stats_after.farming_tokens,
        farming_stats_before
            .farming_tokens
            .checked_add("100".parse().unwrap())
            .unwrap()
    );
}

#[test]
fn test_withdraw() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);
    start_emissions(&market).unwrap();

    market
        .set_time(TimeJump::Seconds((EMISSIONS_DURATION / 4).into()))
        .unwrap();
    farming_deposit(&market, &lp).unwrap();

    market
        .set_time(TimeJump::Seconds((EMISSIONS_DURATION / 2).into()))
        .unwrap();
    farming_withdraw(&market, &lp, None).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();
    let stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(
        stats.emission_rewards,
        LvnToken::from(EMISSIONS_REWARDS.parse::<u64>().unwrap() / 2)
    );
    assert_eq!(stats.farming_tokens, FarmingToken::zero());
}

#[test]
fn test_multiple_withdraws() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);
    start_emissions(&market).unwrap();
    farming_deposit(&market, &lp).unwrap();

    let interval: i64 = (EMISSIONS_DURATION / 4).into();
    market.set_time(TimeJump::Seconds(interval)).unwrap();
    farming_withdraw(&market, &lp, Some("20")).unwrap();

    let stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(stats.emission_rewards, "50".parse().unwrap());
    assert_eq!(stats.farming_tokens, "80".parse().unwrap());

    market.set_time(TimeJump::Seconds(interval)).unwrap();
    farming_withdraw(&market, &lp, Some("80")).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();
    let stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(stats.emission_rewards, "100".parse().unwrap());
    assert_eq!(stats.farming_tokens, FarmingToken::zero());
}

#[test]
fn test_multiple_lps_withdraw() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();
    let interval: i64 = (EMISSIONS_DURATION / 4).into();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);
    start_emissions(&market).unwrap();

    farming_deposit(&market, &lp0).unwrap();

    market.set_time(TimeJump::Seconds(interval)).unwrap();
    farming_deposit(&market, &lp1).unwrap();

    market.set_time(TimeJump::Seconds(interval)).unwrap();
    farming_withdraw(&market, &lp0, None).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();

    let lp0_stats = market.query_farming_farmer_stats(&lp0).unwrap();
    let lp1_stats = market.query_farming_farmer_stats(&lp1).unwrap();

    assert_eq!(lp0_stats.emission_rewards, "75".parse().unwrap());
    assert_eq!(lp1_stats.emission_rewards, "125".parse().unwrap());
}

#[test]
fn test_clear_and_reclaim_emissions() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);

    market.automatic_time_jump_enabled = false;
    move_past_lockdrop(&market);

    let token = start_emissions(&market).unwrap();
    farming_deposit(&market, &lp).unwrap();

    // Jump halfway to the end of the emissions period

    market
        .set_time(TimeJump::Seconds((EMISSIONS_DURATION / 2).into()))
        .unwrap();

    assert!(market.query_farming_status().emissions.is_some());
    market.exec_farming_clear_emissions().unwrap();
    assert!(market.query_farming_status().emissions.is_none());

    // There should be 100 tokens to reclaim, first try 25

    let balance_before = market.query_reward_token_balance(&token, &owner);
    market
        .exec_farming_reclaim_emissions(&owner, Some("25".parse().unwrap()))
        .unwrap();
    let balance_after = market.query_reward_token_balance(&token, &owner);
    assert_eq!(
        balance_after.checked_sub(balance_before).unwrap(),
        "25".parse().unwrap()
    );

    // Assert you can't reclaim more than remains, even if there happens to be more LVN in the contract

    market.mint_lvn_rewards("1000", Some(owner.clone()));
    market
        .exec_farming_reclaim_emissions(&owner, Some("100".parse().unwrap()))
        .unwrap_err();

    // Assert you can reclaim the test

    let balance_before = market.query_reward_token_balance(&token, &owner);
    market.exec_farming_reclaim_emissions(&owner, None).unwrap();
    let balance_after = market.query_reward_token_balance(&token, &owner);
    assert_eq!(
        balance_after.checked_sub(balance_before).unwrap(),
        "75".parse().unwrap()
    )
}

#[test]
fn test_claim_emissions() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();
    let lp2 = market.clone_lp(2).unwrap();

    market.automatic_time_jump_enabled = false;
    move_past_lockdrop(&market);

    // Deposit

    let interval: i64 = (EMISSIONS_DURATION / 4).into();
    let token = start_emissions(&market).unwrap();

    farming_deposit(&market, &lp0).unwrap();

    market.set_time(TimeJump::Seconds(interval)).unwrap();
    farming_deposit(&market, &lp1).unwrap();

    market.set_time(TimeJump::Seconds(interval)).unwrap();
    farming_deposit(&market, &lp2).unwrap();

    // Claim halfway through

    market.exec_farming_claim_emissions(&lp0).unwrap();
    let lp0_balance = market.query_reward_token_balance(&token, &lp0);
    assert_eq!(lp0_balance, "75".parse().unwrap());

    market.exec_farming_claim_emissions(&lp1).unwrap();
    let lp1_balance = market.query_reward_token_balance(&token, &lp1);
    assert_eq!(lp1_balance, "25".parse().unwrap());

    market.exec_farming_claim_emissions(&lp2).unwrap_err();

    // Jump to the end and claim

    market.set_time(TimeJump::Seconds(100)).unwrap();

    market.exec_farming_claim_emissions(&lp0).unwrap();
    let lp0_balance_after = market.query_reward_token_balance(&token, &lp0);
    assert_eq!(
        lp0_balance_after.checked_sub(lp0_balance).unwrap(),
        "33.333333".parse().unwrap()
    );

    market.exec_farming_claim_emissions(&lp1).unwrap();
    let lp1_balance_after = market.query_reward_token_balance(&token, &lp1);
    assert_eq!(
        lp1_balance_after.checked_sub(lp1_balance).unwrap(),
        "33.333333".parse().unwrap()
    );

    market.exec_farming_claim_emissions(&lp2).unwrap();
    let lp2_balance = market.query_reward_token_balance(&token, &lp2);
    assert_eq!(lp2_balance, "33.333333".parse().unwrap());

    let total_lp_balances =
        ((lp0_balance_after + lp1_balance_after).unwrap() + lp2_balance).unwrap();
    assert_eq!(
        EMISSIONS_REWARDS.parse::<f64>().unwrap().round(),
        total_lp_balances
            .to_string()
            .parse::<f64>()
            .unwrap()
            .round()
    );
}

#[test]
fn test_query_farmers() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let market = PerpsMarket::new(app_cell).unwrap();
    let mut farmers: Vec<Addr> = vec![];

    move_past_lockdrop(&market);

    for i in 0..9 {
        let lp = market.clone_lp(i).unwrap();
        farming_deposit(&market, &lp).unwrap();
        farmers.push(lp);
    }

    // Test defaults

    let res = market.query_farmers(None, None).unwrap();
    assert_eq!(res.farmers, farmers);
    assert_eq!(res.next_start_after, None);

    // Full pagination

    let limit = Some(4u32);
    let res = market.query_farmers(None, limit).unwrap();
    assert_eq!(res.farmers, farmers[..4]);
    assert_eq!(res.next_start_after, farmers[3].clone().into());

    let res = market
        .query_farmers(res.next_start_after.map(RawAddr::from), limit)
        .unwrap();
    assert_eq!(res.farmers, farmers[4..8]);
    assert_eq!(res.next_start_after, farmers[7].clone().into());

    let res = market
        .query_farmers(res.next_start_after.map(RawAddr::from), limit)
        .unwrap();
    assert_eq!(res.farmers, farmers[8..]);
    assert_eq!(res.next_start_after, None);
}

#[test]
fn test_farming_status() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let buckets = lockdrop_buckets();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();
    let lp2 = market.clone_lp(2).unwrap();
    let lp3 = market.clone_lp(3).unwrap();

    market.automatic_time_jump_enabled = false;
    market.exec_farming_start_lockdrop(None).unwrap();

    market
        .exec_farming_lockdrop_deposit(&lp0, "100".parse().unwrap(), buckets[0].bucket_id)
        .unwrap();
    market
        .exec_farming_lockdrop_deposit(&lp1, "200".parse().unwrap(), buckets[1].bucket_id)
        .unwrap();
    market
        .exec_farming_lockdrop_deposit(&lp2, "300".parse().unwrap(), buckets[2].bucket_id)
        .unwrap();

    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    market.exec_farming_start_launch().unwrap();
    let started_at = market.now();
    farming_deposit(&market, &lp3).unwrap();

    start_emissions(&market).unwrap();

    let status = market.query_farming_status();
    let lockdrop_buckets = buckets
        .iter()
        .map(|bucket| {
            let duration = bucket.bucket_id.0 * lockdrop_month_seconds();
            let unlocks_at = started_at + Duration::from_seconds(duration.into());
            let deposit = if bucket.bucket_id == buckets[0].bucket_id {
                "100"
            } else if bucket.bucket_id == buckets[1].bucket_id {
                "200"
            } else if bucket.bucket_id == buckets[2].bucket_id {
                "300"
            } else {
                "0"
            }
            .parse()
            .unwrap();

            LockdropBucketStats {
                bucket_id: bucket.bucket_id,
                multiplier: bucket.multiplier,
                deposit,
                unlocks_at: Some(unlocks_at),
            }
        })
        .collect::<Vec<LockdropBucketStats>>();

    let emissions = Some(Emissions {
        start: market.now(),
        end: market.now() + Duration::from_seconds(EMISSIONS_DURATION.into()),
        lvn: EMISSIONS_REWARDS.parse().unwrap(),
    });

    let lockdrop_rewards_unlocked =
        Some(market.now() + Duration::from_seconds(lockdrop_month_seconds().into()));

    assert_eq!(status.period, FarmingPeriodResp::Launched { started_at });
    assert_eq!(status.farming_tokens, "700".parse().unwrap());
    assert_eq!(status.xlp, "700".parse().unwrap());
    assert_eq!(status.lockdrop_buckets, lockdrop_buckets);
    assert_eq!(status.lockdrop_rewards_unlocked, lockdrop_rewards_unlocked);
    assert_eq!(status.emissions, emissions);

    // note: FarmingStatus::bonus is covered in `test_reinvest_yield`
}

#[test]
fn test_farming_update_config() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let market = PerpsMarket::new(app_cell).unwrap();
    let owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
    let new_owner = Addr::unchecked("new_owner");

    market.exec_farming_start_lockdrop(None).unwrap();
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();

    // Test update owner

    market
        .exec_farming(
            &new_owner,
            &ExecuteMsg::Owner(OwnerExecuteMsg::StartLaunchPeriod {}),
        )
        .unwrap_err();

    market
        .exec_farming_update_config(&owner, Some(new_owner.clone().into()), None, None)
        .unwrap();
    market
        .exec_farming(
            &new_owner,
            &ExecuteMsg::Owner(OwnerExecuteMsg::StartLaunchPeriod {}),
        )
        .unwrap();

    // Test update bonus config

    let bonus_ratio = bonus_ratio() + Decimal256::from_ratio(1u64, 10u64);
    let bonus_addr = Addr::unchecked("new_addr");

    market
        .exec_farming_update_config(
            &new_owner,
            None,
            Some(bonus_ratio),
            Some(bonus_addr.clone().into()),
        )
        .unwrap();

    let status = market.query_farming_status();

    assert_eq!(status.bonus_addr, bonus_addr);
    assert_eq!(status.bonus_ratio, bonus_ratio);

    // Test bonus ratio validation

    market
        .exec_farming_update_config(
            &new_owner,
            None,
            Some(Decimal256::from_ratio(2u64, 1u64)),
            None,
        )
        .unwrap_err();
}

#[test]
fn test_reclaimable_emissions_with_gaps() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;
    move_past_lockdrop(&market);

    // test with gap at the beginning

    let token = start_emissions(&market).unwrap();

    market.set_time(TimeJump::Seconds(5)).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();
    let reclaim_addr0 = Addr::unchecked("reclaim_addr0");
    market
        .exec_farming_reclaim_emissions(&reclaim_addr0, None)
        .unwrap();

    let balance = market.query_reward_token_balance(&token, &reclaim_addr0);
    assert_eq!(balance, "50".parse().unwrap());

    // test with gap in the middle

    let token = start_emissions(&market).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds(5)).unwrap();
    farming_withdraw(&market, &lp, None).unwrap();

    market.set_time(TimeJump::Seconds(5)).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();

    let reclaim_addr1 = Addr::unchecked("reclaim_addr1");
    market
        .exec_farming_reclaim_emissions(&reclaim_addr1, None)
        .unwrap();

    let balance = market.query_reward_token_balance(&token, &reclaim_addr1);
    assert_eq!(balance, "50".parse().unwrap());

    // test with gap at the end

    let token = start_emissions(&market).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds(15)).unwrap();
    farming_withdraw(&market, &lp, None).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();

    let reclaim_addr2 = Addr::unchecked("reclaim_addr2");
    market
        .exec_farming_reclaim_emissions(&reclaim_addr2, None)
        .unwrap();

    let balance = market.query_reward_token_balance(&token, &reclaim_addr2);
    assert_eq!(balance, "50".parse().unwrap());

    // test after call to clear emissions

    let token = start_emissions(&market).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds(5)).unwrap();
    farming_withdraw(&market, &lp, None).unwrap();

    market.set_time(TimeJump::Seconds(5)).unwrap();
    market.exec_farming_clear_emissions().unwrap();

    let reclaim_addr3 = Addr::unchecked("reclaim_addr3");
    market
        .exec_farming_reclaim_emissions(&reclaim_addr3, None)
        .unwrap();

    let balance = market.query_reward_token_balance(&token, &reclaim_addr3);
    assert_eq!(balance, "150".parse().unwrap());

    // test after call to clear emissions when emissions are done

    let token = start_emissions(&market).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds(5)).unwrap();
    farming_withdraw(&market, &lp, None).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();
    market.exec_farming_clear_emissions().unwrap();

    let reclaim_addr4 = Addr::unchecked("reclaim_addr4");
    market
        .exec_farming_reclaim_emissions(&reclaim_addr4, None)
        .unwrap();

    let balance = market.query_reward_token_balance(&token, &reclaim_addr4);
    assert_eq!(balance, "150".parse().unwrap());

    // test after two emissions periods

    // ...first emissions
    let token = start_emissions(&market).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds(15)).unwrap();
    farming_withdraw(&market, &lp, None).unwrap();

    market.set_time(TimeJump::Seconds(10)).unwrap();

    // ...second emissions
    start_emissions(&market).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds(15)).unwrap();
    farming_withdraw(&market, &lp, None).unwrap();

    market.set_time(TimeJump::Seconds(10)).unwrap();

    let reclaim_addr5 = Addr::unchecked("reclaim_addr5");
    market
        .exec_farming_reclaim_emissions(&reclaim_addr5, None)
        .unwrap();

    let balance = market.query_reward_token_balance(&token, &reclaim_addr5);
    assert_eq!(balance, "100".parse().unwrap());
}

#[test]
fn test_reclaim_without_deposits() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let reclaim_addr = Addr::unchecked("reclaim-addr");

    market.automatic_time_jump_enabled = false;
    move_past_lockdrop(&market);
    let token = start_emissions(&market).unwrap();

    market.set_time(TimeJump::Seconds(5)).unwrap();
    market
        .exec_farming_reclaim_emissions(&reclaim_addr, None)
        .unwrap();

    market.set_time(TimeJump::Seconds(5)).unwrap();
    market
        .exec_farming_reclaim_emissions(&reclaim_addr, None)
        .unwrap();

    let balance = market.query_reward_token_balance(&token, &reclaim_addr);
    assert_eq!(balance, "100".parse().unwrap())
}

proptest! {
    #![proptest_config(ProptestConfig{
        failure_persistence: None,
        max_shrink_iters: 0,
        max_local_rejects: 1,
        max_global_rejects: 1,
        .. ProptestConfig::with_cases(10)
    })]

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_farming_emissions(
        strategy in FarmingEmissions::new_strategy()
    ) {
        strategy.run().unwrap();
    }
}
