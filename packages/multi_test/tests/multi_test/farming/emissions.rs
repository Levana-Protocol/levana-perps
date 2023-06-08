use crate::prelude::*;
use msg::contracts::farming::events::DepositSource;

const EMISSIONS_DURATION: u32 = 20;
const EMISSIONS_REWARDS: &str = "200";

fn farming_deposit(market: &PerpsMarket, lp: &Addr) -> Result<()> {
    farming_deposit_from_source(market, lp, DepositSource::Xlp)
}

fn farming_withdraw(market: &PerpsMarket, lp: &Addr, amount: Option<&str>) -> Result<()> {
    market.exec_farming_withdraw_xlp(lp, amount.map(|s| s.parse().unwrap()))?;
    Ok(())
}

fn farming_deposit_from_source(market: &PerpsMarket, lp: &Addr, source: DepositSource) -> Result<()> {
    match source {
        DepositSource::Collateral => {
            market
                .exec_farming_deposit_collateral(&lp, "100".parse().unwrap())
                .unwrap();
        }
        DepositSource::Lp => {
            market
                .exec_mint_and_deposit_liquidity(lp, "100".parse().unwrap())?;
            market.exec_farming_deposit_lp(lp, "100".parse().unwrap())?;
        }
        DepositSource::Xlp => {
            market
                .exec_mint_and_deposit_liquidity(lp, "100".parse().unwrap())?;
            market.exec_stake_lp(&lp, Some("100".parse().unwrap()))?;
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

fn start_emissions(market: &PerpsMarket) -> Result<()> {
    let token = market.mint_lvn_rewards(EMISSIONS_REWARDS);
    market
        .exec_farming_set_emissions(market.now(), EMISSIONS_DURATION, EMISSIONS_REWARDS.parse().unwrap(), token.clone())?;

    Ok(())
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
    market.set_time(TimeJump::Seconds(EMISSIONS_DURATION.into())).unwrap();

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
    market.set_time(TimeJump::Seconds((EMISSIONS_DURATION * 3 / 4).into())).unwrap();
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

    let farming_stats_before = market.query_farming_stats();
    farming_deposit_from_source(&market, &lp, DepositSource::Collateral).unwrap();

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

    move_past_lockdrop(&market);

    // Deposit & assert

    let farming_stats_before = market.query_farming_stats();
    farming_deposit_from_source(&market, &lp, DepositSource::Lp).unwrap();

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
fn test_withdraw() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app_cell).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market.automatic_time_jump_enabled = false;

    move_past_lockdrop(&market);
    start_emissions(&market).unwrap();

    market.set_time(TimeJump::Seconds((EMISSIONS_DURATION / 4).into())).unwrap();
    farming_deposit(&market, &lp).unwrap();

    market.set_time(TimeJump::Seconds((EMISSIONS_DURATION / 2).into())).unwrap();
    farming_withdraw(&market, &lp, None).unwrap();

    market.set_time(TimeJump::Seconds(100)).unwrap();
    let stats = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(stats.emission_rewards, LvnToken::from(EMISSIONS_REWARDS.parse::<u64>().unwrap() / 2));
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
    farming_withdraw(&market, &lp, Some("20")).unwrap(); // accrued 40 LVN

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
