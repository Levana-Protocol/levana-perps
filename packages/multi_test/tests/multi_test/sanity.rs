use levana_perpswap_multi_test::return_unless_market_collateral_base;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use perpswap::prelude::*;

#[test]
fn sanity_open_long_min_values() {
    open_position_and_assert(
        100u64.into(),
        "1.1".try_into().unwrap(),
        DirectionToBase::Long,
        "1".try_into().unwrap(),
    );
}

#[test]
fn sanity_open_long_mid_values() {
    open_position_and_assert(
        100u64.into(),
        "15".parse().unwrap(),
        DirectionToBase::Long,
        "45".try_into().unwrap(),
    );
}

#[test]
fn sanity_open_long_max_values() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    if market.id.get_market_type() == MarketType::CollateralIsBase {
        let config = market.query_config().unwrap();
        let leverage = (config.max_leverage - Number::from(5u64)).unwrap();
        open_position_and_assert(
            100u64.into(),
            leverage.to_string().parse().unwrap(),
            DirectionToBase::Long,
            MaxGainsInQuote::PosInfinity,
        );
    }
}

#[test]
fn sanity_open_short_min_values() {
    open_position_and_assert(
        100u64.into(),
        "0.25".try_into().unwrap(),
        DirectionToBase::Short,
        MaxGainsInQuote::Finite("0.01".try_into().unwrap()),
    );
}

#[test]
fn sanity_open_short_mid_values() {
    open_position_and_assert(
        100u64.into(),
        "10".try_into().unwrap(),
        DirectionToBase::Short,
        MaxGainsInQuote::Finite("3".parse().unwrap()),
    );
}

#[test]
fn sanity_open_short_max_values() {
    open_position_and_assert(
        100u64.into(),
        "30".try_into().unwrap(),
        DirectionToBase::Short,
        MaxGainsInQuote::Finite("15".parse().unwrap()),
    );
}

#[test]
fn sanity_update_collateral() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_base!(market);
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "50",
            None,
            None,
            None,
        )
        .unwrap();

    let _res = market
        .exec_update_position_collateral_impact_size(&trader, pos_id, 100u64.into(), None)
        .unwrap();
    let pos = market.query_position(pos_id).unwrap();
    assert_eq!(pos.deposit_collateral, "200".parse().unwrap());
}

#[test]
fn sanity_update_leverage() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_base!(market);

    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "50",
            None,
            None,
            None,
        )
        .unwrap();

    let _res = market
        .exec_update_position_leverage(&trader, pos_id, "20".parse().unwrap(), None)
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();

    // Updated leverage is not going to be exactly 20 because of liquifunding, this is just to ensure it changed and is reasonable
    assert!(pos.leverage.into_number() > "20".parse().unwrap());
    assert!(pos.leverage.into_number() < "21".parse().unwrap());
}

#[test]
fn sanity_spot_price() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let price_resp = market.query_current_price().unwrap();
    assert_eq!(
        price_resp.price_usd.into_number(),
        price_resp.price_notional.into_number()
    );

    let old_price_usd = price_resp.price_usd.into_number();

    let new_price: NumberGtZero = "4.2".try_into().unwrap();
    market
        .exec_set_price(PriceBaseInQuote::try_from_number(new_price.into()).unwrap())
        .unwrap();

    let price_resp = market.query_current_price().unwrap();

    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            assert!(price_resp
                .price_notional
                .into_number()
                .approx_eq(new_price.into())
                .unwrap());
        }
        MarketType::CollateralIsBase => {
            assert!(price_resp
                .price_notional
                .into_number()
                .approx_eq(new_price.inverse().into())
                .unwrap());
        }
    }

    return_unless_market_collateral_base!(&market);
    assert!(!price_resp
        .price_usd
        .into_number()
        .approx_eq(old_price_usd)
        .unwrap());
}

fn open_position_and_assert(
    collateral: Number,
    leverage: LeverageToBase,
    direction: DirectionToBase,
    max_gains: MaxGainsInQuote,
) {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_base!(market);

    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    // need additional lp
    market
        .exec_mint_and_deposit_liquidity(&lp, 1_000_000_000u128.into())
        .unwrap();

    let (pos_id, _) = market
        .exec_open_position_raw(
            &trader, collateral, None, leverage, direction, max_gains, None, None,
        )
        .unwrap();

    market.query_position(pos_id).unwrap();
}
