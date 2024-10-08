use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{compat::BackwardsCompatTakeProfit, prelude::*};

#[derive(Debug)]
struct OpenParam {
    collateral: Number,
    leverage: LeverageToBase,
    max_gains: MaxGainsInQuote,
}

#[test]
fn max_gain_edge() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    // This will always fail because max gains is exceeded
    let position = market.exec_open_position(
        &trader,
        "10",
        "5",
        DirectionToBase::Long,
        "200000000000000000000000000000000000000000000000000000000000",
        None,
        None,
        None,
    );
    assert!(position.is_err());

    let high_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "5".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "1099999999999999999999999999999999999999999999999999999999.99"
                .parse()
                .unwrap(),
        },
    };

    // This should succeed as this is the highest max gains where it
    // works
    market
        .exec_open_position_raw(
            &trader,
            high_param.collateral,
            None,
            high_param.leverage,
            DirectionToBase::Long,
            high_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let low_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.17".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.1725".parse().unwrap(),
        },
    };

    // This should succeed as this is the lowest max gain where it
    // works
    market
        .exec_open_position_raw(
            &trader,
            low_param.collateral,
            None,
            low_param.leverage,
            DirectionToBase::Long,
            low_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let low_param_fail = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.16".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.1724".parse().unwrap(),
        },
    };

    // Going lower than the lowest max gain will result in the position opening with a higher take profit price
    let (pos_id, _) = market
        .exec_open_position_raw(
            &trader,
            low_param_fail.collateral,
            None,
            low_param_fail.leverage,
            DirectionToBase::Long,
            low_param_fail.max_gains,
            None,
            None,
        )
        .unwrap();

    let price = market.query_current_price().unwrap();
    let take_profit_price_requested = BackwardsCompatTakeProfit {
        leverage: low_param_fail.leverage,
        direction: DirectionToBase::Long,
        collateral: NonZero::new(Collateral::try_from_number(low_param_fail.collateral).unwrap())
            .unwrap(),
        market_type,
        max_gains: low_param_fail.max_gains,
        take_profit: None,
        price_point: &price,
    }
    .calc()
    .unwrap();

    let take_profit_price_requested = match take_profit_price_requested {
        TakeProfitTrader::Finite(value) => value,
        TakeProfitTrader::PosInfinity => panic!("expected finite take profit price"),
    }
    .into_number();

    let pos = market.query_position(pos_id).unwrap();

    let take_profit_trader = pos
        .take_profit_trader
        .unwrap()
        .as_finite()
        .unwrap()
        .into_number();

    let take_profit_price_position = pos.take_profit_total_base.unwrap().into_number();
    assert!(take_profit_price_requested < take_profit_price_position);
    assert_eq!(take_profit_price_requested, take_profit_trader);

    // sanity check that this wasn't accidental
    let (pos_id, _) = market
        .exec_open_position_raw(
            &trader,
            low_param.collateral,
            None,
            low_param.leverage,
            DirectionToBase::Long,
            low_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let price = market.query_current_price().unwrap();
    let take_profit_price_requested = BackwardsCompatTakeProfit {
        leverage: low_param.leverage,
        direction: DirectionToBase::Long,
        collateral: NonZero::new(Collateral::try_from_number(low_param.collateral).unwrap())
            .unwrap(),
        market_type,
        max_gains: low_param.max_gains,
        take_profit: None,
        price_point: &price,
    }
    .calc()
    .unwrap();

    let take_profit_price_requested = match take_profit_price_requested {
        TakeProfitTrader::Finite(value) => value,
        TakeProfitTrader::PosInfinity => panic!("expected finite take profit price"),
    }
    .into_number();

    let pos = market.query_position(pos_id).unwrap();
    let take_profit_trader = pos
        .take_profit_trader
        .unwrap()
        .as_finite()
        .unwrap()
        .into_number();

    let take_profit_price_position = pos.take_profit_total_base.unwrap().into_number();
    assert_eq!(take_profit_price_requested, take_profit_price_position);
    assert_eq!(take_profit_price_requested, take_profit_trader);
}

#[test]
fn leverage_edge() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let high_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "30".parse().unwrap(),
            max_gains: "1.0".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "30".parse().unwrap(),
            max_gains: "500".parse().unwrap(),
        },
    };

    // The maximum value of leverage for which you can successfully
    // open a position
    market
        .exec_open_position_raw(
            &trader,
            high_param.collateral,
            None,
            high_param.leverage,
            DirectionToBase::Long,
            high_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let high_param_fail = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "31".parse().unwrap(),
            max_gains: "1.0".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "31".parse().unwrap(),
            max_gains: "500".parse().unwrap(),
        },
    };

    // Pushing beyond maximum value of leverage will result in failure
    let response = market.exec_open_position_raw(
        &trader,
        high_param_fail.collateral,
        None,
        high_param_fail.leverage,
        DirectionToBase::Long,
        high_param_fail.max_gains,
        None,
        None,
    );
    assert!(response.is_err());

    let low_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "0.2".parse().unwrap(),
            max_gains: "0.17".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "1.00001".parse().unwrap(),
            max_gains: "500".parse().unwrap(),
        },
    };

    // The minimum value of leverage for which you can successfully
    // open a position
    market
        .exec_open_position_raw(
            &trader,
            low_param.collateral,
            None,
            low_param.leverage,
            DirectionToBase::Long,
            low_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let low_param_fail = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "0.1".parse().unwrap(),
            max_gains: "0.17".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "0.9".parse().unwrap(),
            max_gains: "500".parse().unwrap(),
        },
    };

    // Pushing below the minimum value of leverage will result in
    // failure
    let response = market.exec_open_position_raw(
        &trader,
        low_param_fail.collateral,
        None,
        low_param_fail.leverage,
        DirectionToBase::Long,
        low_param_fail.max_gains,
        None,
        None,
    );
    // Funky test, because it's still using the legacy max gains API,
    // we can get an error from the max gains API. However, we do not
    // want to accept any counterleverage errors. So: this either succeeds
    // or specifically talks about max gains.
    assert!(
        response.is_ok()
            || response
                .unwrap_err()
                .to_string()
                .contains("Max gains are too large")
    )
}

#[test]
fn collateral_edge() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let high_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "9999".parse().unwrap(),
            leverage: "0.2".parse().unwrap(),
            max_gains: "0.17".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "99999999".parse().unwrap(),
            leverage: "1.00001".parse().unwrap(),
            max_gains: "5".parse().unwrap(),
        },
    };

    // The maximum value of collateral for which you can successfully
    // open a position
    market
        .exec_open_position_raw(
            &trader,
            high_param.collateral,
            None,
            high_param.leverage,
            DirectionToBase::Long,
            high_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let high_param_fail = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "99999".parse().unwrap(),
            leverage: "0.2".parse().unwrap(),
            max_gains: "0.17".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "999999999".parse().unwrap(),
            leverage: "1.00001".parse().unwrap(),
            max_gains: "5".parse().unwrap(),
        },
    };

    // Pushing values beyond the maximum value permissible results in
    // error
    let response = market.exec_open_position_raw(
        &trader,
        high_param_fail.collateral,
        None,
        high_param_fail.leverage,
        DirectionToBase::Long,
        high_param_fail.max_gains,
        None,
        None,
    );
    assert!(response.is_err());

    let low_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "5".parse().unwrap(),
            leverage: "0.2".parse().unwrap(),
            max_gains: "0.17".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "5".parse().unwrap(),
            leverage: "1.00002".parse().unwrap(),
            max_gains: "5".parse().unwrap(),
        },
    };

    // Lowest collateral using which we can successfully open
    // positions
    market
        .exec_open_position_raw(
            &trader,
            low_param.collateral,
            None,
            low_param.leverage,
            DirectionToBase::Long,
            low_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let low_param_fail = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "4".parse().unwrap(),
            leverage: "0.2".parse().unwrap(),
            max_gains: "0.17".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "4".parse().unwrap(),
            leverage: "1.00002".parse().unwrap(),
            max_gains: "5".parse().unwrap(),
        },
    };

    // Opening positions below the allowed lowest collateral will
    // result in an error
    let response = market.exec_open_position_raw(
        &trader,
        low_param_fail.collateral,
        None,
        low_param_fail.leverage,
        DirectionToBase::Long,
        low_param.max_gains,
        None,
        None,
    );
    assert!(response.is_err());
}
