use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, time::TimeJump, PerpsApp};
use msg::contracts::market::config::ConfigUpdate;
use msg::prelude::*;

fn enable_crank_fee(market: &PerpsMarket) -> anyhow::Result<()> {
    market.exec_set_config(ConfigUpdate {
        crank_fee_charged: Some("0.01".parse()?),
        crank_fee_reward: Some("0.001".parse()?),
        ..Default::default()
    })?;
    Ok(())
}

#[test]
fn crank_fee_is_charged_and_paid() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    enable_crank_fee(&market).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = Addr::unchecked("cranker");

    // Get starting token balance for the cranker
    assert_eq!(
        market.query_collateral_balance(&cranker).unwrap(),
        Number::ZERO
    );
    assert_eq!(market.query_fees().unwrap().crank, Collateral::zero());
    assert_eq!(
        market.query_lp_info(&cranker).unwrap().available_yield,
        Collateral::zero()
    );

    // Open a position. We still don't have any crank fees, that will happen after we liquifund.
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "5",
            "5",
            DirectionToBase::Long,
            "2",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(market.query_fees().unwrap().crank, Collateral::zero());
    assert_eq!(
        market.query_position(pos_id).unwrap().crank_fee_collateral,
        Collateral::zero()
    );

    // Cranking with liquifunding should charge a crank fee and allocate to the cranker
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_refresh_price().unwrap();
    assert_eq!(
        market.query_collateral_balance(&cranker).unwrap(),
        Number::ZERO
    );
    assert_eq!(
        market.query_lp_info(&cranker).unwrap().available_yield,
        Collateral::zero()
    );
    market.exec_crank_till_finished(&cranker).unwrap();
    let fees_after_crank = market.query_fees().unwrap().crank;
    let crank_fees_pending = market.query_lp_info(&cranker).unwrap().available_yield;
    let config = market.query_config().unwrap();
    assert_eq!(
        // our current collateral-to-usd price is 1, so just compare directly
        (fees_after_crank + crank_fees_pending).into_decimal256(),
        config.crank_fee_charged.into_decimal256(),
        "fees_after_crank: {fees_after_crank}. crank_fees_pending: {crank_fees_pending}. charged: {}", config.crank_fee_charged
    );
    // The cranking above did multiple operations, not just our liquifunding. Therefore we check that we have _at least_ the reward amount.
    assert!(
        crank_fees_pending.into_decimal256() >= config.crank_fee_reward.into_decimal256(),
        "Fee reward: {}. Pending: {crank_fees_pending}",
        config.crank_fee_reward
    );
    market.exec_claim_yield(&cranker).unwrap();
    let cranker_balance_after_crank = market.query_collateral_balance(&cranker).unwrap();
    assert_eq!(
        cranker_balance_after_crank,
        crank_fees_pending.into_number()
    );

    // Cranking again shouldn't transfer any more funds
    market.exec_crank_till_finished(&cranker).unwrap();
    let fees_after_crank2 = market.query_fees().unwrap().crank;
    assert_eq!(fees_after_crank, fees_after_crank2);
    assert_eq!(
        market.query_lp_info(&cranker).unwrap().available_yield,
        Collateral::zero()
    );
    market.exec_claim_yield(&cranker).unwrap_err();

    // Rely on a collateral price of 1 USD
    assert_eq!(
        market
            .query_position(pos_id)
            .unwrap()
            .crank_fee_collateral
            .into_decimal256(),
        market
            .query_config()
            .unwrap()
            .crank_fee_charged
            .into_decimal256()
    );
}

#[test]
fn fund_crank_pool() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    enable_crank_fee(&market).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let funder = Addr::unchecked("funder");

    assert_eq!(market.query_fees().unwrap().crank, Collateral::zero());
    market
        .exec_mint_tokens(&funder, "1000".parse().unwrap())
        .unwrap();
    market
        .exec_provide_crank_funds(&funder, "1000".parse().unwrap())
        .unwrap();
    assert_eq!(market.query_fees().unwrap().crank, "1000".parse().unwrap());

    // And operations work afterwards, basically making sure we pass sanity checks
    market
        .exec_open_position(
            &trader,
            "5",
            "5",
            DirectionToBase::Long,
            "2",
            None,
            None,
            None,
        )
        .unwrap();
}

#[test]
fn crank_fee_is_charged_for_limit_orders() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    enable_crank_fee(&market).unwrap();
    let trader = market.clone_trader(0).unwrap();

    assert_eq!(market.query_fees().unwrap().crank, Collateral::zero());
    market
        .exec_place_limit_order(
            &trader,
            "10".parse().unwrap(),
            "0.9".parse().unwrap(),
            "10".parse().unwrap(),
            DirectionToBase::Long,
            "2".parse().unwrap(),
            None,
            None,
        )
        .unwrap();
    assert_ne!(market.query_fees().unwrap().crank, Collateral::zero());
}

#[test]
fn mismatched_crank_fee_in_collateral_perp_980() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    enable_crank_fee(&market).unwrap();
    let trader = market.clone_trader(0).unwrap();

    assert_eq!(market.query_fees().unwrap().crank, Collateral::zero());
    market
        .exec_open_position(
            &trader,
            "5",
            "5",
            DirectionToBase::Long,
            "2",
            None,
            None,
            None,
        )
        .unwrap();

    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_set_price("1.01".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
}

#[test]
fn crank_fee_to_rewards_wallet() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    enable_crank_fee(&market).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = Addr::unchecked("cranker");
    let rewards = Addr::unchecked("rewards");

    // Get starting token balance for the cranker and rewards
    assert_eq!(
        market.query_collateral_balance(&cranker).unwrap(),
        Number::ZERO
    );
    assert_eq!(
        market.query_collateral_balance(&rewards).unwrap(),
        Number::ZERO
    );
    assert_eq!(market.query_fees().unwrap().crank, Collateral::zero());
    assert_eq!(
        market.query_lp_info(&cranker).unwrap().available_yield,
        Collateral::zero()
    );

    // Open a position. We still don't have any crank fees, that will happen after we liquifund.
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "5",
            "5",
            DirectionToBase::Long,
            "2",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(market.query_fees().unwrap().crank, Collateral::zero());
    assert_eq!(
        market.query_position(pos_id).unwrap().crank_fee_collateral,
        Collateral::zero()
    );

    // Cranking with liquifunding should charge a crank fee and allocate to the cranker
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_refresh_price().unwrap();
    assert_eq!(
        market.query_collateral_balance(&cranker).unwrap(),
        Number::ZERO
    );
    assert_eq!(
        market.query_lp_info(&cranker).unwrap().available_yield,
        Collateral::zero()
    );
    market
        .exec_crank_till_finished_with_rewards(&cranker, &rewards)
        .unwrap();
    let fees_after_crank = market.query_fees().unwrap().crank;

    assert_eq!(
        market.query_lp_info(&cranker).unwrap().available_yield,
        Collateral::zero()
    );

    let crank_fees_pending = market.query_lp_info(&rewards).unwrap().available_yield;
    let config = market.query_config().unwrap();
    assert_eq!(
        // our current collateral-to-usd price is 1, so just compare directly
        (fees_after_crank + crank_fees_pending).into_decimal256(),
        config.crank_fee_charged.into_decimal256(),
        "fees_after_crank: {fees_after_crank}. crank_fees_pending: {crank_fees_pending}. charged: {}", config.crank_fee_charged
    );
    // The cranking above did multiple operations, not just our liquifunding. Therefore we check that we have _at least_ the reward amount.
    assert!(
        crank_fees_pending.into_decimal256() >= config.crank_fee_reward.into_decimal256(),
        "Fee reward: {}. Pending: {crank_fees_pending}",
        config.crank_fee_reward
    );
    market.exec_claim_yield(&cranker).unwrap_err();
    market.exec_claim_yield(&rewards).unwrap();
    let cranker_balance_after_crank = market.query_collateral_balance(&rewards).unwrap();
    assert_eq!(
        cranker_balance_after_crank,
        crank_fees_pending.into_number()
    );
}
