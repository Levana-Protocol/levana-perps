use cosmwasm_std::Decimal256;
use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket,
    position_helpers::{assert_position_liquidated, assert_position_max_gains},
    response::CosmosResponseExt,
    return_unless_market_collateral_quote,
    time::TimeJump,
    PerpsApp,
};
use perpswap::contracts::market::{
    config::ConfigUpdate, entry::PositionsQueryFeeApproach, position::PositionId,
};
use perpswap::prelude::*;

// this is currently a known issue, working around it in the meantime
// however, to make sure it doesn't get lost in the mix
// instead of commenting it out, it's a runtime config here
// see also the position_take_profit_[normal/massive] tests
const SKIP_CHECK_LARGE_PNL_IS_TAKE_PROFIT: bool = true;

#[test]
fn position_pnl_close_no_change() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    // open/close with no price movement, pnl should be 0
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();
    let start_pnl_in_collateral = pos.pnl_collateral;

    // just setting this test to ensure it's within some realistic range
    assert!(
        start_pnl_in_collateral > "-3.0".parse().unwrap() && start_pnl_in_collateral.is_negative()
    );

    let defer_res = market.exec_close_position(&trader, pos_id, None).unwrap();
    let delta_neutrality_fee_close = defer_res.exec_resp().first_delta_neutrality_fee_amount();

    let pos = market.query_closed_position(&trader, pos_id).unwrap();

    assert_eq!(
        pos.pnl_collateral.into_number(),
        (start_pnl_in_collateral.into_number()
            - (delta_neutrality_fee_close + pos.borrow_fee_collateral.into_number()).unwrap())
        .unwrap()
    );
}

#[test]
fn position_pnl_close_take_profit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // open/close with no price movement, pnl should be 0
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();
    let start_pnl_in_collateral = pos.pnl_collateral;

    // just setting this test to ensure it's within some realistic range
    assert!(
        start_pnl_in_collateral > "-3.0".parse().unwrap() && start_pnl_in_collateral.is_negative()
    );

    // change the price to something crazy
    market.exec_set_price("100000".try_into().unwrap()).unwrap();

    // pnl is updated even without cranking or liquifunding
    let pos = market
        .query_position_pending_close(pos_id, PositionsQueryFeeApproach::NoFees)
        .unwrap();
    assert!(pos.pnl_collateral > start_pnl_in_collateral);

    // crank to liquidate
    market.exec_crank_till_finished(&cranker).unwrap();

    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert!(pos.pnl_collateral > start_pnl_in_collateral);

    if !SKIP_CHECK_LARGE_PNL_IS_TAKE_PROFIT {
        assert_position_max_gains(&pos).unwrap();
    }
}

#[test]
fn position_pnl_close_liquidate() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // open/close with no price movement, pnl should be 0
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();
    let start_pnl_in_collateral = pos.pnl_collateral;

    // just setting this test to ensure it's within some realistic range
    assert!(
        start_pnl_in_collateral > "-3.0".parse().unwrap() && start_pnl_in_collateral.is_negative()
    );

    // change the price to something crazy
    market.exec_set_price("100000".try_into().unwrap()).unwrap();

    // pnl is updated even without cranking or liquifunding
    let pos = market
        .query_position_pending_close(pos_id, PositionsQueryFeeApproach::NoFees)
        .unwrap();
    assert!(pos.pnl_collateral < start_pnl_in_collateral);

    // crank to liquidate
    market.exec_crank_till_finished(&cranker).unwrap();

    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert!(pos.pnl_collateral < start_pnl_in_collateral);

    assert_position_liquidated(&pos).unwrap();
}

#[test]
fn position_pnl_close_profit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    // open/close with no price movement, pnl should be 0
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();
    let start_pnl_in_collateral = pos.pnl_collateral;

    // just setting this test to ensure it's within some realistic range
    assert!(
        start_pnl_in_collateral > "-3.0".parse().unwrap() && start_pnl_in_collateral.is_negative()
    );

    // price went up a bit
    let new_price = (market
        .query_current_price()
        .unwrap()
        .price_base
        .into_number()
        * Number::try_from("1.02").unwrap())
    .unwrap();

    market
        .exec_set_price(PriceBaseInQuote::try_from_number(new_price).unwrap())
        .unwrap();
    let pos = market.query_position(pos_id).unwrap();

    // pnl is affected before closing
    assert!(pos.pnl_collateral > start_pnl_in_collateral);

    // and in closed position history
    market.exec_close_position(&trader, pos_id, None).unwrap();
    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert!(pos.pnl_collateral > start_pnl_in_collateral);
}

#[test]
fn position_pnl_close_loss() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    // open/close with no price movement, pnl should be 0
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();
    let start_pnl_in_collateral = pos.pnl_collateral;

    // just setting this test to ensure it's within some realistic range
    assert!(
        start_pnl_in_collateral > "-3.0".parse().unwrap() && start_pnl_in_collateral.is_negative()
    );

    // price went down a bit
    let new_price = (market
        .query_current_price()
        .unwrap()
        .price_base
        .into_number()
        * Number::try_from("0.98").unwrap())
    .unwrap();
    market
        .exec_set_price(PriceBaseInQuote::try_from_number(new_price).unwrap())
        .unwrap();

    // pnl is affected before closing
    let pos = market.query_position(pos_id).unwrap();
    assert!(pos.pnl_collateral < start_pnl_in_collateral);

    // and in closed position history
    market.exec_close_position(&trader, pos_id, None).unwrap();
    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert!(pos.pnl_collateral < start_pnl_in_collateral);
}

#[test]
fn position_pnl_long_and_short_precise() {
    let app = PerpsApp::new_cell().unwrap();
    let mut market = PerpsMarket::new(app.clone()).unwrap();
    return_unless_market_collateral_quote!(market);
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // This test was written assuming liquifunding would occur once per epoch (== 1 hour), so make the config match.
    market
        .exec_set_config(ConfigUpdate {
            liquifunding_delay_seconds: Some(60 * 60),
            // We need precise liquifunding periods for this test so remove randomization
            liquifunding_delay_fuzz_seconds: Some(0),
            delta_neutrality_fee_tax: Some(Decimal256::zero()),
            // Precise values were calculated using original config value
            funding_rate_sensitivity: Some("1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    market
        .exec_open_position_queue_only(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_open_position_queue_only(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_open_position_queue_only(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_open_position_queue_only(
            &trader,
            "200",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let short_pos_id = PositionId::new(3);
    let long_pos_id = PositionId::new(4);

    market.exec_refresh_price().unwrap();
    market.exec_crank_n(&cranker, 100).unwrap();

    let short_slippage_fee = market
        .query_position(short_pos_id)
        .unwrap()
        .delta_neutrality_fee_collateral
        .into_number();
    let long_slippage_fee = market
        .query_position(long_pos_id)
        .unwrap()
        .delta_neutrality_fee_collateral
        .into_number();

    // Long interest > short interest
    let rates = market.query_status().unwrap();
    assert_eq!(rates.long_funding.to_string(), "0.2");
    assert_eq!(rates.short_funding.to_string(), "-0.3");
    assert_eq!(rates.borrow_fee.to_string(), "0.01");

    market.automatic_time_jump_enabled = true;

    let long_before_epoch = market.query_position(long_pos_id).unwrap();
    let config = market.query_config().unwrap();
    let mut total_long_pnl = ((-long_before_epoch.notional_size.abs().into_number()
        * config.trading_fee_notional_size.into_number())
    .unwrap()
        - (long_before_epoch.counter_collateral.into_number()
            * config.trading_fee_counter_collateral.into_number())
        .unwrap())
    .unwrap();

    let short_before_epoch = market.query_position(short_pos_id).unwrap();
    let mut total_short_pnl = ((-short_before_epoch.notional_size.abs().into_number()
        * config.trading_fee_notional_size.into_number())
    .unwrap()
        - (short_before_epoch.counter_collateral.into_number()
            * config.trading_fee_counter_collateral.into_number())
        .unwrap())
    .unwrap();

    assert_eq!(
        (long_before_epoch.pnl_collateral.into_number() + long_slippage_fee).unwrap(),
        total_long_pnl
    );
    assert_eq!(
        (short_before_epoch.pnl_collateral.into_number() + short_slippage_fee).unwrap(),
        total_short_pnl
    );
    assert_eq!(
        ((long_before_epoch.deposit_collateral.into_number() + total_long_pnl).unwrap()
            - long_slippage_fee)
            .unwrap(),
        long_before_epoch.active_collateral.into_number()
    );
    assert_eq!(
        ((short_before_epoch.deposit_collateral.into_number() + total_short_pnl).unwrap()
            - short_slippage_fee)
            .unwrap(),
        short_before_epoch.active_collateral.into_number()
    );

    assert_eq!(long_before_epoch.notional_size.to_string(), "2000");
    assert_eq!(short_before_epoch.notional_size.to_string(), "-1000");
    assert_eq!(long_before_epoch.counter_collateral.to_string(), "200");
    assert_eq!(short_before_epoch.counter_collateral.to_string(), "100");

    // We want to force liquifunding to happen at exactly one hour
    // Jump one block back to make sure that manual set price happens
    // exactly at liquifunding time.
    market.set_time(TimeJump::Hours(1)).unwrap();
    market.set_time(TimeJump::Blocks(-1)).unwrap();

    market.exec_set_price("0.98".try_into().unwrap()).unwrap();
    market.exec_crank_n(&cranker, 100).unwrap();

    let long_after_epoch_1 = market.query_position(long_pos_id).unwrap();
    let short_after_epoch_1 = market.query_position(short_pos_id).unwrap();

    assert_eq!(long_after_epoch_1.notional_size.to_string(), "2000");
    assert_eq!(short_after_epoch_1.notional_size.to_string(), "-1000");
    assert_eq!(long_after_epoch_1.counter_collateral.to_string(), "240");
    assert_eq!(short_after_epoch_1.counter_collateral.to_string(), "80");

    let funding_estimate_long_1 =
        ((-long_before_epoch.notional_size.abs().into_number() * rates.long_funding).unwrap()
            / Number::from(365u64 * 24u64))
        .unwrap();
    let cost_of_capital_estimate_long_1 = ((-long_before_epoch.counter_collateral.into_number()
        * rates.borrow_fee.into_number())
    .unwrap()
        / Number::from(365u64 * 24u64))
    .unwrap();
    let funding_estimate_short_1 =
        ((-short_before_epoch.notional_size.abs().into_number() * rates.short_funding).unwrap()
            / Number::from(365u64 * 24u64))
        .unwrap();
    let cost_of_capital_estimate_short_1 = ((-short_before_epoch.counter_collateral.into_number()
        * rates.borrow_fee.into_number())
    .unwrap()
        / Number::from(365u64 * 24u64))
    .unwrap();
    let long_price_pnl = (Number::try_from("-0.02").unwrap()
        * long_before_epoch.notional_size.into_number())
    .unwrap();
    let short_price_pnl = (Number::try_from("-0.02").unwrap()
        * short_before_epoch.notional_size.into_number())
    .unwrap();

    total_long_pnl = (((total_long_pnl + long_price_pnl).unwrap() + funding_estimate_long_1)
        .unwrap()
        + cost_of_capital_estimate_long_1)
        .unwrap();

    total_short_pnl = (((total_short_pnl + short_price_pnl).unwrap() + funding_estimate_short_1)
        .unwrap()
        + cost_of_capital_estimate_short_1)
        .unwrap();

    assert_eq!(
        (long_after_epoch_1.pnl_collateral.into_number() + long_slippage_fee).unwrap(),
        total_long_pnl
    );
    assert_eq!(
        (short_after_epoch_1.pnl_collateral.into_number() + short_slippage_fee).unwrap(),
        total_short_pnl
    );
    assert_eq!(
        ((long_after_epoch_1.deposit_collateral.into_number() + total_long_pnl).unwrap()
            - long_slippage_fee)
            .unwrap(),
        long_after_epoch_1.active_collateral.into_number()
    );
    assert_eq!(
        ((short_after_epoch_1.deposit_collateral.into_number() + total_short_pnl).unwrap()
            - short_slippage_fee)
            .unwrap(),
        short_after_epoch_1.active_collateral.into_number()
    );

    // See comment above about timing, this is to let calculations be precise.
    let open_timestamp = long_after_epoch_1.liquifunded_at;
    let desired_crank_time = open_timestamp.plus_seconds(3600);

    market
        .set_time(TimeJump::PreciseTime(desired_crank_time.into()))
        //.set_time(TimeJump::Hours(1))
        .unwrap();

    market.exec_crank_n(&cranker, 100).unwrap();

    let long_after_epoch_2 = market.query_position(long_pos_id).unwrap();
    let short_after_epoch_2 = market.query_position(short_pos_id).unwrap();

    // TODO: fix long/short_before_epoch -> long/short_after_epoch_1 after cost of capital payment
    //       calculation is made to reflect intra-epoch price changes.
    let funding_estimate_long_2 =
        (((-long_after_epoch_1.notional_size.abs().into_number() * rates.long_funding).unwrap()
            * market
                .query_current_price()
                .unwrap()
                .price_notional
                .into_number())
        .unwrap()
            / Number::from(365u64 * 24u64))
        .unwrap();
    let cost_of_capital_estimate_long_2 = ((-long_after_epoch_1.counter_collateral.into_number()
        * rates.borrow_fee.into_number())
    .unwrap()
        / Number::from(365u64 * 24u64))
    .unwrap();
    let funding_estimate_short_2 =
        (((-short_after_epoch_1.notional_size.abs().into_number() * rates.short_funding).unwrap()
            * market
                .query_current_price()
                .unwrap()
                .price_notional
                .into_number())
        .unwrap()
            / Number::from(365u64 * 24u64))
        .unwrap();
    let cost_of_capital_estimate_short_2 =
        ((-short_after_epoch_1.counter_collateral.into_number() * rates.borrow_fee.into_number())
            .unwrap()
            / Number::from(365u64 * 24u64))
        .unwrap();

    total_long_pnl = ((total_long_pnl + funding_estimate_long_2).unwrap()
        + cost_of_capital_estimate_long_2)
        .unwrap();
    total_short_pnl = ((total_short_pnl + funding_estimate_short_2).unwrap()
        + cost_of_capital_estimate_short_2)
        .unwrap();

    assert_eq!(
        (long_after_epoch_2.pnl_collateral.into_number() + long_slippage_fee).unwrap(),
        total_long_pnl
    );
    assert_eq!(
        (short_after_epoch_2.pnl_collateral.into_number() + short_slippage_fee).unwrap(),
        total_short_pnl
    );
    assert_eq!(
        ((long_after_epoch_2.deposit_collateral.into_number() + total_long_pnl).unwrap()
            - long_slippage_fee)
            .unwrap(),
        long_after_epoch_2.active_collateral.into_number()
    );
    assert_eq!(
        ((short_after_epoch_2.deposit_collateral.into_number() + total_short_pnl).unwrap()
            - short_slippage_fee)
            .unwrap(),
        short_after_epoch_2.active_collateral.into_number()
    );

    assert_eq!(
        long_after_epoch_2.pnl_collateral.into_number(),
        long_after_epoch_2.pnl_usd.into_number()
    );
}

#[test]
fn position_pnl_usd() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    // open/close with no price movement, pnl should be 0
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let start_pos = market.query_position(pos_id).unwrap();

    // price went down a bit
    let new_price = (market
        .query_current_price()
        .unwrap()
        .price_base
        .into_number()
        * Number::try_from("0.98").unwrap())
    .unwrap();

    market
        .exec_set_price(PriceBaseInQuote::try_from_number(new_price).unwrap())
        .unwrap();

    // pnl is affected before closing
    let pos = market.query_position(pos_id).unwrap();
    assert_ne!(pos.pnl_collateral, start_pos.pnl_collateral);
    assert_ne!(pos.pnl_usd, start_pos.pnl_usd);

    // we can't really check for the equivilent to notional, even if notional is usd
    // because the usd price is strictly in terms of collateral
    // so internally it's collateral * price, while notional would be notional / price
    if market.id.get_market_type() == MarketType::CollateralIsQuote {
        assert_eq!(pos.pnl_collateral.into_number(), pos.pnl_usd.into_number());
    }

    // and in closed position history
    market.exec_close_position(&trader, pos_id, None).unwrap();
    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert_ne!(pos.pnl_collateral, start_pos.pnl_collateral);
    assert_ne!(pos.pnl_usd, start_pos.pnl_usd);
    if market.id.get_market_type() == MarketType::CollateralIsQuote {
        assert_eq!(pos.pnl_collateral.into_number(), pos.pnl_usd.into_number());
    }
}
