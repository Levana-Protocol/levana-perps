//! PERP-808

use levana_perpswap_multi_test::{
    config::{TokenKind, DEFAULT_MARKET, TEST_CONFIG},
    market_wrapper::PerpsMarket,
    PerpsApp,
};
use msg::prelude::*;
use msg::token::TokenInit;

/// Special market setup for these tests, which leverages a non-USD quote to
/// avoid inconsistent price errors.
fn custom_market_setup() -> anyhow::Result<PerpsMarket> {
    let app = PerpsApp::new_cell()?;
    let market_id = match DEFAULT_MARKET.collateral_type {
        MarketType::CollateralIsQuote => "ETH_BTC",
        MarketType::CollateralIsBase => "ETH+_BTC",
    }
    .parse()?;
    let token_init = match DEFAULT_MARKET.token_kind {
        TokenKind::Native => TokenInit::Native {
            denom: TEST_CONFIG.native_denom.to_string(),
            decimal_places: 6,
        },
        TokenKind::Cw20 => {
            let addr = app
                .borrow_mut()
                .get_cw20_addr(&DEFAULT_MARKET.cw20_symbol)?;
            TokenInit::Cw20 { addr: addr.into() }
        }
    };
    PerpsMarket::new_custom(
        app,
        market_id,
        token_init,
        "1".parse()?,
        Some("1".parse()?),
        true,
    )
}

#[test]
fn too_little_collateral_open() {
    let market = custom_market_setup().unwrap();
    let trader = market.clone_trader(0).unwrap();

    // Too little
    market
        .exec_open_position(
            &trader,
            "2",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap_err();

    // Just barely enough
    market
        .exec_open_position(
            &trader,
            "5",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    // Changing the base/quote price has no impact
    market
        .exec_set_price_with_usd("0.1".parse().unwrap(), Some("1".parse().unwrap()))
        .unwrap();
    market
        .exec_open_position(
            &trader,
            "2",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap_err();
    market
        .exec_set_price_with_usd("10".parse().unwrap(), Some("1".parse().unwrap()))
        .unwrap();
    market
        .exec_open_position(
            &trader,
            "2",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap_err();

    // Now change the price so that 2 is more than 5 dollars.
    market
        .exec_set_price_with_usd("2.5".parse().unwrap(), Some("2.5".parse().unwrap()))
        .unwrap();
    market
        .exec_open_position(
            &trader,
            "2",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();
}

#[test]
fn too_little_collateral_update() {
    let market = custom_market_setup().unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            // see test below, we allow a 10% discrepancy so we set this just over 4.5
            "4.55",
            "10",
            DirectionToBase::Long,
            "3.0",
            None,
            None,
            None,
        )
        .unwrap();

    // Cannot withdraw enough collateral to go below the limit
    market
        .exec_update_position_collateral_impact_size(&trader, pos_id, "-0.2".parse().unwrap(), None)
        .unwrap_err();
    market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "-0.2".parse().unwrap())
        .unwrap_err();

    // Move the price in our direction, now we can withdraw some funds
    market
        .exec_set_price_with_usd("1.1".parse().unwrap(), Some("1".parse().unwrap()))
        .unwrap();
    market
        .exec_update_position_collateral_impact_size(&trader, pos_id, "-0.2".parse().unwrap(), None)
        .unwrap();
}

#[test]
fn update_without_reduction_is_fine() {
    let market = custom_market_setup().unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "5",
            "10",
            DirectionToBase::Long,
            "3.0",
            None,
            None,
            None,
        )
        .unwrap();

    // Move price against us, assert that we have less than 5 collateral.
    market
        .exec_set_price_with_usd("0.95".parse().unwrap(), Some("1".parse().unwrap()))
        .unwrap();
    assert!(market.query_position(pos_id).unwrap().active_collateral < "5".parse().unwrap());

    // We should be able to update something like max gains
    market
        .exec_update_position_max_gains(&trader, pos_id, "2.5".parse().unwrap())
        .unwrap();

    // We should also be able to deposit more collateral, even if it isn't enough to pass the 5 limit
    market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "0.02".parse().unwrap())
        .unwrap();
    assert!(market.query_position(pos_id).unwrap().active_collateral < "5".parse().unwrap());
}

#[test]
fn allow_some_price_variance() {
    let market = custom_market_setup().unwrap();
    let trader = market.clone_trader(0).unwrap();

    // We allow up to 10% (hard-coded value) variability. So $4.50 should be
    // allowed, and $4.49 should not.
    market
        .exec_open_position(
            &trader,
            "4.49",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap_err();
    market
        .exec_open_position(
            &trader,
            "4.5",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();
}
