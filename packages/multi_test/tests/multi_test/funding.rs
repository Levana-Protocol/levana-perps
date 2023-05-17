use levana_perpswap_multi_test::return_unless_market_collateral_quote;
use levana_perpswap_multi_test::time::TimeJump;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::contracts::market::config::{Config, ConfigUpdate};
use msg::prelude::*;

#[test]
fn funding_rates_typical() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();

    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    // No short interest
    let rates = market.query_status().unwrap();
    assert_eq!(rates.long_funding.to_string(), "0");
    assert_eq!(rates.short_funding.to_string(), "0");
    assert_eq!(rates.borrow_fee.to_string(), "0.01");

    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    // Longs balance shorts
    let rates = market.query_status().unwrap();

    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            assert_eq!(rates.long_funding.to_string(), "0");
            assert_eq!(rates.short_funding.to_string(), "0");
            assert_eq!(rates.borrow_fee.to_string(), "0.01");
        }
        MarketType::CollateralIsBase => {
            let expected = Number::from_str("-0.122222").unwrap();
            assert!(
                rates.long_funding.approx_eq_eps(expected, Number::EPS_E6),
                "long_funding_base {} does not equal {}",
                rates.long_funding,
                expected
            );
            assert_eq!(rates.short_funding, Number::from_str("0.1").unwrap());
            assert_eq!(rates.borrow_fee, Decimal256::from_str("0.01").unwrap());
        }
    }

    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    // Short interest > long interest
    let rates = market.query_status().unwrap();

    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            assert_eq!(rates.long_funding.to_string(), "-0.666666666666666666");
            assert_eq!(rates.short_funding.to_string(), "0.333333333333333333");
            assert_eq!(rates.borrow_fee.to_string(), "0.01");
        }
        MarketType::CollateralIsBase => {
            let expected_long_rate = Number::from_str("-1.025089").unwrap();
            assert!(
                rates
                    .long_funding
                    .approx_eq_eps(expected_long_rate, Number::EPS_E6),
                "long_funding_base {} does not equal {}",
                rates.long_funding,
                expected_long_rate
            );

            let expected_short_rate = Number::from_str("0.419354").unwrap();
            assert!(
                rates
                    .short_funding
                    .approx_eq_eps(expected_short_rate, Number::EPS_E6),
                "short_funding_base {} does not equal {}",
                rates.short_funding,
                expected_short_rate
            );

            assert_eq!(rates.borrow_fee, Decimal256::from_str("0.01").unwrap());
        }
    }

    let (last_pos_id, _) = market
        .exec_open_position(
            &trader,
            "200",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    // Long interest > short interest
    let rates = market.query_status().unwrap();

    //create match on market type
    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            assert_eq!(rates.long_funding.to_string(), "0.2");
            assert_eq!(rates.short_funding.to_string(), "-0.3");
            assert_eq!(rates.borrow_fee.to_string(), "0.01");
        }
        MarketType::CollateralIsBase => {
            let expected_long_rate = Number::from_str("0.102040").unwrap();
            assert!(
                rates
                    .long_funding
                    .approx_eq_eps(expected_long_rate, Number::EPS_E6),
                "long_funding_base {} does not equal {}",
                rates.long_funding,
                expected_long_rate
            );

            let expected_short_rate = Number::from_str("-0.125231").unwrap();
            assert!(
                rates
                    .short_funding
                    .approx_eq_eps(expected_short_rate, Number::EPS_E6),
                "short_funding_base {} does not equal {}",
                rates.short_funding,
                expected_short_rate
            );

            assert_eq!(rates.borrow_fee, Decimal256::from_str("0.01").unwrap());
        }
    }

    market
        .exec_close_position(&trader, last_pos_id, None)
        .unwrap();

    // Rates back to what they were before last position opened
    let rates = market.query_status().unwrap();

    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            assert_eq!(rates.long_funding.to_string(), "-0.666666666666666666");
            assert_eq!(rates.short_funding.to_string(), "0.333333333333333333");
            assert_eq!(rates.borrow_fee.to_string(), "0.01");
        }
        MarketType::CollateralIsBase => {
            let expected_long_rate = Number::from_str("-1.025089").unwrap();
            assert!(
                rates
                    .long_funding
                    .approx_eq_eps(expected_long_rate, Number::EPS_E6),
                "long_funding_base {} does not equal {}",
                rates.long_funding,
                expected_long_rate
            );

            let expected_short_rate = Number::from_str("0.419354").unwrap();
            assert!(
                rates
                    .short_funding
                    .approx_eq_eps(expected_short_rate, Number::EPS_E6),
                "short_funding_base {} does not equal {}",
                rates.short_funding,
                expected_short_rate
            );

            assert_eq!(rates.borrow_fee, Decimal256::from_str("0.01").unwrap());
        }
    }
}

#[test]
fn funding_payment_typical() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_quote!(market);

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();
    // Ensure that the liquidation margin amount matches the old epoch rules for calculations below
    market
        .exec_set_config(ConfigUpdate {
            liquifunding_delay_seconds: Some(40 * 60),
            // We need precise liquifunding periods for this test so remove randomization
            liquifunding_delay_fuzz_seconds: Some(0),
            staleness_seconds: Some(20 * 60),
            ..Default::default()
        })
        .unwrap();

    market.set_time(TimeJump::Hours(2)).unwrap();
    market.exec_refresh_price().unwrap();

    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    let (short_pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    let (long_pos_id, _) = market
        .exec_open_position(
            &trader,
            "200",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    let long_before_epoch = market.query_position(long_pos_id).unwrap();
    let short_before_epoch = market.query_position(short_pos_id).unwrap();

    // Long interest > short interest
    let rates = market.query_status().unwrap();
    market.set_time(TimeJump::Hours(2)).unwrap();
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    let long_after_epoch = market.query_position(long_pos_id).unwrap();
    let short_after_epoch = market.query_position(short_pos_id).unwrap();

    // long pos; 365 days, 24 hours, but 2 epochs of time.
    let funding_estimate = -long_before_epoch.notional_size.abs().into_number()
        * rates.long_funding
        / Number::from(365u64 * 24u64 / 2u64);
    let borrow_fee = -long_before_epoch.counter_collateral.into_number()
        * rates.borrow_fee.into_number()
        / Number::from(365u64 * 24u64 / 2u64);
    assert!(
        (long_after_epoch.active_collateral.into_number() - long_before_epoch.active_collateral.into_number())
            .approx_eq(funding_estimate + borrow_fee),
        "after active: {}, before active: {}, delta: {}, funding estimate: {funding_estimate}, borrow fee: {borrow_fee}. Off by: {}",
        long_after_epoch.active_collateral,
        long_before_epoch.active_collateral,
        long_after_epoch.active_collateral.into_number() - long_before_epoch.active_collateral.into_number(),
        long_after_epoch.active_collateral.into_number() - long_before_epoch.active_collateral.into_number()
            - funding_estimate - borrow_fee,
    );

    assert!(long_after_epoch
        .funding_fee_collateral
        .into_number()
        .approx_eq(-funding_estimate));
    assert_ne!(long_after_epoch.funding_fee_usd, Signed::zero());
    assert!(long_after_epoch
        .borrow_fee_collateral
        .into_number()
        .approx_eq(-borrow_fee));
    assert_ne!(long_after_epoch.borrow_fee_usd, Usd::zero());

    // short pos; 365 days, 24 hours, but 2 epochs of time.
    let funding_estimate = -short_before_epoch.notional_size.abs().into_number()
        * rates.short_funding
        / Number::from(365u64 * 24u64 / 2u64);
    let borrow_fee = -short_before_epoch.counter_collateral.into_number()
        * rates.borrow_fee.into_number()
        / Number::from(365u64 * 24u64 / 2u64);
    assert!((short_after_epoch.active_collateral.into_number()
        - short_before_epoch.active_collateral.into_number())
    .approx_eq(funding_estimate + borrow_fee));

    assert!(short_after_epoch
        .funding_fee_collateral
        .into_number()
        .approx_eq(-funding_estimate));
    assert_ne!(short_after_epoch.funding_fee_usd, Signed::zero());
    assert!(short_after_epoch
        .borrow_fee_collateral
        .into_number()
        .approx_eq(-borrow_fee));
    assert_ne!(short_after_epoch.borrow_fee_usd, Usd::zero());
}

#[test]
fn funding_borrow_fee() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market
        .exec_set_config(ConfigUpdate {
            protocol_tax: Some(Decimal256::zero()),
            ..Default::default()
        })
        .unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // Ensure that the liquidation margin amount matches the old epoch rules for calculations below
    market
        .exec_set_config(ConfigUpdate {
            liquifunding_delay_seconds: Some(60 * 60),
            // We need precise liquifunding periods for this test so remove randomization
            liquifunding_delay_fuzz_seconds: Some(0),
            staleness_seconds: Some(20 * 60),
            ..Default::default()
        })
        .unwrap();

    // Open some positions

    let (pos1_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    let (pos2_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "11",
            DirectionToBase::Short,
            "1.2",
            None,
            None,
            None,
        )
        .unwrap();

    // Get starting fees

    let fees_before = market.query_fees().unwrap();

    // Get expected borrow fee after liquifunding

    let pos1 = market.query_position(pos1_id).unwrap();
    let pos2 = market.query_position(pos2_id).unwrap();
    let rates = market.query_status().unwrap();
    let pos1_borrow_fee = pos1.counter_collateral.into_number() * rates.borrow_fee.into_number()
        / Number::from(365u64 * 24u64);
    let pos2_borrow_fee = pos2.counter_collateral.into_number() * rates.borrow_fee.into_number()
        / Number::from(365u64 * 24u64);

    let expected_borrow_fees_balance = fees_before.wallets
        + Collateral::try_from_number(pos1_borrow_fee).unwrap()
        + Collateral::try_from_number(pos2_borrow_fee).unwrap();

    // Trigger liquifunding

    market.set_time(TimeJump::Hours(1)).unwrap();
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    // Get actual yield fund balance and assert

    let fees_after = market.query_fees().unwrap();

    assert_eq!(expected_borrow_fees_balance, fees_after.wallets);

    // check that our estimates match the actual
    let pos1 = market.query_position(pos1_id).unwrap();
    let pos2 = market.query_position(pos2_id).unwrap();
    assert_eq!(pos1.borrow_fee_collateral.into_number(), pos1_borrow_fee);
    assert_ne!(pos1.borrow_fee_usd, Usd::zero());
    assert_eq!(pos2.borrow_fee_collateral.into_number(), pos2_borrow_fee);
    assert_ne!(pos2.borrow_fee_usd, Usd::zero());
}

#[test]
fn insolvency_crank_stall_perp_787() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_quote!(market);
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let cranker = Addr::unchecked("cranker");

    market
        .exec_set_config(ConfigUpdate {
            minimum_deposit_usd: Some("0".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    market
        .exec_set_config(ConfigUpdate {
            delta_neutrality_fee_cap: Some("0.05".try_into().unwrap()),
            ..Default::default()
        })
        .unwrap();
    market
        .exec_mint_and_deposit_liquidity(&lp, Number::try_from("100000000000000").unwrap())
        .unwrap();

    // Open up a long and a short position
    // where shorts pay longs for funding payments
    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_open_position(
            &trader,
            "1000",
            "10",
            DirectionToBase::Short,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    // delay the crank a bit
    market
        .set_time(TimeJump::FractionalLiquifundings(0.25))
        .unwrap();
    // set the price crazy high
    market.exec_set_price("1000000".parse().unwrap()).unwrap();

    // FIXME
    // trigger updating the funding rate on data series
    market
        .exec_open_position(
            &trader,
            "1",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    // jump forward a bit more
    market
        .set_time(TimeJump::FractionalLiquifundings(0.5))
        .unwrap();
    market.exec_refresh_price().unwrap();
    // boom
    market.exec_crank_till_finished(&cranker).unwrap();
}

#[test]
fn funding_rates_capped() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    market
        .exec_mint_and_deposit_liquidity(&trader, "1000000000".parse().unwrap())
        .unwrap();

    // Temporarily set delta_neutrality_fee_sensitivity very high to open very large positions without hitting validation errors.
    market
        .exec_set_config(ConfigUpdate {
            delta_neutrality_fee_sensitivity: Some("1000000000000".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    market
        .exec_open_position(
            &trader,
            "1000000",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_open_position(
            &trader,
            "2000000",
            "10",
            DirectionToBase::Short,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_set_config(ConfigUpdate {
            delta_neutrality_fee_sensitivity: Some(
                Config::default().delta_neutrality_fee_sensitivity,
            ),
            ..Default::default()
        })
        .unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    let rates = market.query_status().unwrap();

    // Much higher sensitivity after hitting the cap will result in capped funding rates.
    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            assert_eq!(rates.long_funding.to_string(), "-1.8");
            assert_eq!(rates.short_funding.to_string(), "0.9");
            assert_eq!(rates.borrow_fee.to_string(), "0.01");
        }
        MarketType::CollateralIsBase => {
            let expected_long_rate = Number::from_str("-2.2").unwrap();
            assert!(
                rates
                    .long_funding
                    .approx_eq_eps(expected_long_rate, Number::EPS_E6),
                "long_funding_base {} does not equal {}",
                rates.long_funding,
                expected_long_rate
            );

            let expected_short_rate = Number::from_str("0.9").unwrap();
            assert!(
                rates
                    .short_funding
                    .approx_eq_eps(expected_short_rate, Number::EPS_E6),
                "short_funding_base {} does not equal {}",
                rates.short_funding,
                expected_short_rate
            );

            assert_eq!(rates.borrow_fee, Decimal256::from_str("0.01").unwrap());
        }
    }
}
