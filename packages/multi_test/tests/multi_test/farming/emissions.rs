use crate::prelude::*;
use levana_perpswap_multi_test::config::TEST_CONFIG;

#[test]
fn test_emissions() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;
    market
        .exec_mint_and_deposit_liquidity(&lp, "100".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&lp, None).unwrap();

    market.exec_farming_start_lockdrop(None).unwrap();
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    market.exec_farming_start_launch().unwrap();

    let amount = "200";
    let token = market.setup_lvn_rewards(amount);

    // sanity check
    let protocol_owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
    let balance = market.query_reward_token_balance(&token, &protocol_owner);
    assert_eq!(balance, LvnToken::from_str(amount).unwrap());

    market
        .exec_farming_set_emissions(market.now(), 20, amount.parse().unwrap(), token)
        .unwrap();

    // Test query farming rewards

    market
        .exec_farming_deposit_xlp(&lp, NonZero::new("100".parse().unwrap()).unwrap())
        .unwrap();

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

    market.exec_farming_start_lockdrop(None).unwrap();
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    market.exec_farming_start_launch().unwrap();

    let amount = "200";
    let token = market.setup_lvn_rewards(amount);
    market
        .exec_farming_set_emissions(market.now(), 20, amount.parse().unwrap(), token)
        .unwrap();

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
fn test_deposit_collateral() {
    // Setup

    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;

    market.exec_farming_start_lockdrop(None).unwrap();
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    market.exec_farming_start_launch().unwrap();

    // Deposit & assert

    let farming_stats_before = market.query_farming_stats();
    market
        .exec_farming_deposit_collateral(&lp, "100".parse().unwrap())
        .unwrap();

    let farmer_stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(farmer_stats.farming_tokens, "100".parse().unwrap());

    let farming_stats_after = market.query_farming_stats();
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

    market.exec_farming_start_lockdrop(None).unwrap();
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    market.exec_farming_start_launch().unwrap();

    // Deposit & assert

    let farming_stats_before = market.query_farming_stats();
    market
        .exec_farming_deposit_lp(&lp, "100".parse().unwrap())
        .unwrap();

    let farmer_stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(farmer_stats.farming_tokens, "100".parse().unwrap());

    let farming_stats_after = market.query_farming_stats();
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
