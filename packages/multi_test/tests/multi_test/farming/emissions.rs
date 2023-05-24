use crate::prelude::*;

#[test]
fn test_emissions() {
    // Setup

    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.automatic_time_jump_enabled = false;

    let lp = market.clone_lp(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&lp, "100".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&lp, None).unwrap();

    market.exec_farming_start_lockdrop(None).unwrap();
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    market.exec_farming_start_launch(None).unwrap();

    market
        .exec_farming_set_emissions(market.now(), 20, "200".parse().unwrap())
        .unwrap();

    // Test claim farming rewards

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

    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
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
    market.exec_farming_start_launch(None).unwrap();

    market
        .exec_farming_set_emissions(market.now(), 20, "200".parse().unwrap())
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
