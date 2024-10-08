use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, time::TimeJump, PerpsApp};
use perpswap::contracts::market::{entry::SlippageAssert, liquidity::LiquidityStats};
use perpswap::prelude::*;

fn test_position_open_inner(
    market: PerpsMarket,
    direction: DirectionToBase,
    time_jump: Option<TimeJump>,
    expected_liquidity: &LiquidityStats,
) {
    let trader = market.clone_trader(0).unwrap();

    if let Some(time_jump) = time_jump {
        market.set_time(time_jump).unwrap();
        market.exec_refresh_price().unwrap();
    }

    let (pos_id, _) = market
        .exec_open_position(&trader, "100", "9", direction, "1.0", None, None, None)
        .unwrap();

    // liquidity is adjusted due to open
    let liquidity = market.query_liquidity_stats().unwrap();

    liquidity.approx_eq(expected_liquidity);

    // sanity check that the NFT works
    let nft_ids = market.query_position_token_ids(&trader).unwrap();
    assert_eq!(nft_ids.len(), 1);

    // sanity check that positions query works
    let pos = market.query_position(pos_id).unwrap();

    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 1);

    assert_eq!(positions[0].id, pos_id);
    assert_eq!(pos.id, pos_id);
}

#[test]
fn test_position_open_long() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let expected_liquidity = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => LiquidityStats {
            locked: "100".parse().unwrap(),
            unlocked: "2900".parse().unwrap(),
            total_lp: "3000".parse().unwrap(),
            total_xlp: "0".parse().unwrap(),
        },
        MarketType::CollateralIsBase => LiquidityStats {
            locked: "80".parse().unwrap(),
            unlocked: "2920".parse().unwrap(),
            total_lp: "3000".parse().unwrap(),
            total_xlp: "0".parse().unwrap(),
        },
    };

    test_position_open_inner(market, DirectionToBase::Long, None, &expected_liquidity);

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    test_position_open_inner(
        market,
        DirectionToBase::Long,
        Some(TimeJump::Liquifundings(3)),
        &expected_liquidity,
    );
}

#[test]
fn test_position_open_short() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let expected_liquidity = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => LiquidityStats {
            locked: "100".parse().unwrap(),
            unlocked: "2900".parse().unwrap(),
            total_lp: "3000".parse().unwrap(),
            total_xlp: "0".parse().unwrap(),
        },
        MarketType::CollateralIsBase => LiquidityStats {
            locked: "125".parse().unwrap(),
            unlocked: "2875".parse().unwrap(),
            total_lp: "3000".parse().unwrap(),
            total_xlp: "0".parse().unwrap(),
        },
    };

    test_position_open_inner(market, DirectionToBase::Short, None, &expected_liquidity);

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    test_position_open_inner(
        market,
        DirectionToBase::Short,
        Some(TimeJump::Liquifundings(3)),
        &expected_liquidity,
    );
}

#[test]
fn position_open_fail() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(0).unwrap();

    let balance_before_open = market.query_collateral_balance(&trader).unwrap();

    market
        .exec_open_position(
            &trader,
            "3000",
            "20",
            DirectionToBase::Long,
            "+Inf",
            None,
            None,
            None,
        )
        .unwrap_err();

    market.exec_crank(&cranker).unwrap();

    market
        .exec_open_position(
            &trader,
            "3000",
            "20",
            DirectionToBase::Short,
            "+Inf",
            None,
            None,
            None,
        )
        .unwrap_err();

    market.exec_crank(&cranker).unwrap();

    let balance_after_open = market.query_collateral_balance(&trader).unwrap();
    assert_eq!(balance_before_open, balance_after_open);
}

#[test]
fn position_open_slippage_assert() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let price = market.query_current_price().unwrap().price_notional;

    market
        .exec_mint_and_deposit_liquidity(&trader, "2000".parse().unwrap())
        .unwrap();

    market
        .exec_open_position(
            &trader,
            "10",
            "20",
            DirectionToBase::Short,
            "1",
            Some(SlippageAssert {
                price: PriceBaseInQuote::try_from_number(
                    (price.into_number() * Number::try_from("1.05").unwrap()).unwrap(),
                )
                .unwrap(),
                tolerance: Number::try_from("0.01").unwrap(),
            }),
            None,
            None,
        )
        .unwrap_err();

    let pos = market
        .exec_open_position(
            &trader,
            "10",
            "20",
            DirectionToBase::Short,
            "1",
            Some(SlippageAssert {
                price: PriceBaseInQuote::try_from_number(
                    (price.into_number() * Number::try_from("1.01").unwrap()).unwrap(),
                )
                .unwrap(),
                tolerance: Number::try_from("0.05").unwrap(),
            }),
            None,
            None,
        )
        .unwrap();
    market.exec_close_position(&trader, pos.0, None).unwrap();

    let pos = market
        .exec_open_position(
            &trader,
            "10",
            "20",
            DirectionToBase::Long,
            "1",
            Some(SlippageAssert {
                price: PriceBaseInQuote::try_from_number(
                    (price.into_number() * Number::try_from("1.05").unwrap()).unwrap(),
                )
                .unwrap(),
                tolerance: Number::try_from("0.01").unwrap(),
            }),
            None,
            None,
        )
        .unwrap();
    market.exec_close_position(&trader, pos.0, None).unwrap();

    for direction in [DirectionToBase::Short, DirectionToBase::Long] {
        market.exec_crank_till_finished(&trader).unwrap();

        let price_point = market.query_current_price().unwrap();
        let leverage_to_base = LeverageToBase::try_from("20")
            .unwrap()
            .into_signed(direction);
        let leverage_to_notional = leverage_to_base
            .into_notional(market.id.get_market_type())
            .unwrap();
        let notional_size_in_collateral = leverage_to_notional
            .checked_mul_collateral(NonZero::new(Collateral::try_from("1000").unwrap()).unwrap())
            .unwrap();
        let notional_size =
            notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x));

        let fee = market
            .query_slippage_fee(notional_size.into_number(), None)
            .unwrap()
            .amount
            .into_number();

        let fee_rate = (fee / notional_size.abs().into_number()).unwrap();
        market
            .exec_open_position(
                &trader,
                "1000",
                "20",
                direction,
                "1",
                Some(SlippageAssert {
                    price: PriceBaseInQuote::try_from_number(price.into_number()).unwrap(),
                    tolerance: (fee_rate * Number::try_from("0.9").unwrap()).unwrap(),
                }),
                None,
                None,
            )
            .unwrap_err();

        let pos = market
            .exec_open_position(
                &trader,
                "1000",
                "20",
                direction,
                "1",
                Some(SlippageAssert {
                    price: PriceBaseInQuote::try_from_number(price.into_number()).unwrap(),
                    tolerance: (fee_rate * Number::try_from("1.1").unwrap()).unwrap(),
                }),
                None,
                None,
            )
            .unwrap();

        market.exec_close_position(&trader, pos.0, None).unwrap();
    }
}

#[test]
fn position_open_slippage_assert_exact_queried() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&trader, "2000".parse().unwrap())
        .unwrap();

    for direction in [DirectionToBase::Short, DirectionToBase::Long] {
        let price_point = market.query_current_price().unwrap();
        let leverage_to_base = LeverageToBase::try_from("20")
            .unwrap()
            .into_signed(direction);
        let leverage_to_notional = leverage_to_base
            .into_notional(market.id.get_market_type())
            .unwrap();
        let notional_size_in_collateral = leverage_to_notional
            .checked_mul_collateral(NonZero::new(Collateral::try_from("1000").unwrap()).unwrap())
            .unwrap();
        let notional_size =
            notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x));

        let slippage_price = market
            .query_slippage_fee(notional_size.into_number(), None)
            .unwrap()
            .slippage_assert_price;
        market
            .exec_open_position(
                &trader,
                "1000",
                "20",
                direction,
                "1",
                Some(SlippageAssert {
                    price: slippage_price,
                    tolerance: Number::try_from("-0.000001").unwrap(),
                }),
                None,
                None,
            )
            .unwrap_err();

        market
            .exec_open_position(
                &trader,
                "1000",
                "20",
                direction,
                "1",
                Some(SlippageAssert {
                    price: slippage_price,
                    tolerance: Number::try_from("0.000001").unwrap(),
                }),
                None,
                None,
            )
            .unwrap();
    }
}
