use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{contracts::market::position::PositionId, prelude::*};

#[derive(Debug)]
struct OpenParam {
    collateral: Number,
    leverage: LeverageToBase,
    max_gains: MaxGainsInQuote,
}

#[test]
fn take_profit_edge() {
    // The overall idea of this test is to open a position and then test that we can update the take_profit price
    // as expected around the edges of maximum and minimum values
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    #[derive(Debug)]
    struct Edges {
        min: Edge,
        max: Edge,
    }

    #[derive(Debug)]
    struct Edge {
        // helps debugging
        #[allow(dead_code)]
        value: Option<Decimal256>,
        // helps debugging
        #[allow(dead_code)]
        side: Side,
        valid: Option<TakeProfitTrader>,
        invalid: Option<TakeProfitTrader>,
    }

    #[derive(Debug, Clone, Copy)]
    enum Side {
        Min,
        Max,
    }

    impl Edge {
        fn infinity(side: Side) -> Self {
            Self {
                value: None,
                side,
                valid: Some(TakeProfitTrader::PosInfinity),
                invalid: None,
            }
        }
        fn new(edge_value: Decimal256, side: Side) -> Self {
            match NonZero::new(edge_value) {
                None => {
                    Self {
                        value: Some(edge_value),
                        side,
                        valid: match side {
                            // min is zero, but that's not even valid on the type level... test explicit very low price right above that min
                            Side::Min => Some("0.0000001".parse().unwrap()),
                            Side::Max => {
                                panic!("max of zero leaves no room to set any price")
                            }
                        },
                        // can't express a 0 price to test
                        invalid: None,
                    }
                }
                Some(value) => {
                    // test the prices just a bit above and below the edge
                    let partial_value =
                        NonZero::new(Decimal256::from_ratio(1u32, 3u32) * value.into_decimal256())
                            .unwrap();
                    match side {
                        Side::Min => Self {
                            side,
                            value: Some(edge_value),
                            valid: Some(TakeProfitTrader::Finite(
                                value.checked_add(partial_value.into_decimal256()).unwrap(),
                            )),
                            invalid: Some(TakeProfitTrader::Finite(
                                value.checked_sub(partial_value.into_decimal256()).unwrap(),
                            )),
                        },
                        Side::Max => Self {
                            side,
                            value: Some(edge_value),
                            valid: Some(TakeProfitTrader::Finite(
                                value.checked_sub(partial_value.into_decimal256()).unwrap(),
                            )),
                            invalid: Some(TakeProfitTrader::Finite(
                                value.checked_add(partial_value.into_decimal256()).unwrap(),
                            )),
                        },
                    }
                }
            }
        }

        fn assert(&self, position_id: PositionId, trader: &Addr, market: &PerpsMarket) {
            if let Some(valid) = self.valid {
                let response = market.exec_update_position_take_profit(trader, position_id, valid);
                assert!(response.is_ok());
            }
            if let Some(invalid) = self.invalid {
                let response =
                    market.exec_update_position_take_profit(trader, position_id, invalid);
                assert!(response.is_err());
            }
        }
    }

    impl Edges {
        fn new(direction: DirectionToBase, market: &PerpsMarket) -> Self {
            let direction_to_notional = direction.into_notional(market.id.get_market_type());
            let price = market.query_current_price().unwrap();
            let price_notional = price.price_notional.into_number();
            match direction_to_notional {
                DirectionToNotional::Short => {
                    let min = Decimal256::zero();
                    let max = price_notional.abs_unsigned();

                    match market.id.get_market_type() {
                        MarketType::CollateralIsQuote => Edges {
                            min: Edge::new(min, Side::Min),
                            max: Edge::new(max, Side::Max),
                        },
                        MarketType::CollateralIsBase => Edges {
                            min: Edge::new(Decimal256::one() / max, Side::Min),
                            max: Edge::infinity(Side::Max),
                        },
                    }
                }
                DirectionToNotional::Long => {
                    let min = price_notional.abs_unsigned();
                    let max = min * Decimal256::from_ratio(2u32, 1u32);

                    match market.id.get_market_type() {
                        MarketType::CollateralIsQuote => Edges {
                            min: Edge::new(min, Side::Min),
                            max: Edge::new(max, Side::Max),
                        },
                        MarketType::CollateralIsBase => Edges {
                            min: Edge::new(Decimal256::one() / max, Side::Min),
                            max: Edge::new(Decimal256::one() / min, Side::Max),
                        },
                    }
                }
            }
        }

        fn assert(&self, position_id: PositionId, trader: &Addr, market: &PerpsMarket) {
            self.min.assert(position_id, trader, market);
            self.max.assert(position_id, trader, market);
        }
    }

    let open_long = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.2".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "200".parse().unwrap(),
        },
    };

    let (position_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_long.collateral,
            None,
            open_long.leverage,
            DirectionToBase::Long,
            open_long.max_gains,
            None,
            None,
        )
        .unwrap();

    Edges::new(DirectionToBase::Long, &market).assert(position_id, &trader, &market);

    let open_short = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "1.0".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.2".parse().unwrap(),
        },
    };
    let (position_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_short.collateral,
            None,
            open_short.leverage,
            DirectionToBase::Short,
            open_short.max_gains,
            None,
            None,
        )
        .unwrap();

    Edges::new(DirectionToBase::Short, &market).assert(position_id, &trader, &market);
}

#[test]
fn leverage_edge() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let open_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.2".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "200".parse().unwrap(),
        },
    };

    let (position_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param.collateral,
            None,
            open_param.leverage,
            DirectionToBase::Long,
            open_param.max_gains,
            None,
            None,
        )
        .unwrap();

    // Max value of leverage where update is possible
    let high_leverage = "29";

    market
        .exec_update_position_leverage(&trader, position_id, high_leverage.parse().unwrap(), None)
        .unwrap();

    // Bumping above high_leverage will result in failure
    let fail_high_leverage = "30.01";

    let response = market.exec_update_position_leverage(
        &trader,
        position_id,
        fail_high_leverage.parse().unwrap(),
        None,
    );

    assert!(response.is_err());

    // Min value of leverage where update is possible
    let low_leverage = match market_type {
        MarketType::CollateralIsQuote => "0.0000009",
        MarketType::CollateralIsBase => "2",
    };

    market
        .exec_update_position_leverage(&trader, position_id, low_leverage.parse().unwrap(), None)
        .unwrap();

    // Value below low_leverage should cause it to fail
    let fail_low_leverage = match market_type {
        MarketType::CollateralIsQuote => "0.00000009",
        MarketType::CollateralIsBase => "1",
    };

    let response = market.exec_update_position_leverage(
        &trader,
        position_id,
        fail_low_leverage.parse().unwrap(),
        None,
    );

    assert!(response.is_err())
}

#[test]
fn collateral_edge_leverage() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let open_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.2".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "200".parse().unwrap(),
        },
    };

    let (position_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param.collateral,
            None,
            open_param.leverage,
            DirectionToBase::Long,
            open_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let low_param_collateral = "-5".parse().unwrap();

    market
        .exec_update_position_collateral_impact_leverage(&trader, position_id, low_param_collateral)
        .unwrap();

    let low_fail_param_collateral = "-1".parse().unwrap();

    let response = market.exec_update_position_collateral_impact_leverage(
        &trader,
        position_id,
        low_fail_param_collateral,
    );

    assert!(
        response.is_err(),
        "Lower leverage than 0 results in failure"
    );

    let (position_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param.collateral,
            None,
            open_param.leverage,
            DirectionToBase::Long,
            open_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let max_param_collateral = "99999999";

    market
        .exec_update_position_collateral_impact_leverage(
            &trader,
            position_id,
            max_param_collateral.parse().unwrap(),
        )
        .unwrap();
    let fail_max_param_collateral = "999999999";
    let response = market.exec_update_position_collateral_impact_leverage(
        &trader,
        position_id,
        fail_max_param_collateral.parse().unwrap(),
    );
    assert!(
        response.is_err(),
        "Bumping beyond max param results in failure"
    );
}

#[test]
fn collateral_edge_size() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&trader, "5000".parse().unwrap())
        .unwrap();

    let open_param = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "0.2".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "10".parse().unwrap(),
            leverage: "5".parse().unwrap(),
            max_gains: "200".parse().unwrap(),
        },
    };

    let (position_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param.collateral,
            None,
            open_param.leverage,
            DirectionToBase::Long,
            open_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let low_param_collateral = "-5".parse().unwrap();

    market
        .exec_update_position_collateral_impact_size(
            &trader,
            position_id,
            low_param_collateral,
            None,
        )
        .unwrap();

    let low_fail_param_collateral = "-1".parse().unwrap();

    let response = market.exec_update_position_collateral_impact_size(
        &trader,
        position_id,
        low_fail_param_collateral,
        None,
    );

    assert!(
        response.is_err(),
        "Lower leverage than 0 results in failure"
    );

    let (position_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param.collateral,
            None,
            open_param.leverage,
            DirectionToBase::Long,
            open_param.max_gains,
            None,
            None,
        )
        .unwrap();

    let max_param_collateral = match market_type {
        MarketType::CollateralIsQuote => "9999",
        MarketType::CollateralIsBase => "999",
    };

    market
        .exec_update_position_collateral_impact_size(
            &trader,
            position_id,
            max_param_collateral.parse().unwrap(),
            None,
        )
        .unwrap();

    let fail_max_param_collateral = match market_type {
        MarketType::CollateralIsQuote => "9999",
        MarketType::CollateralIsBase => "999",
    };
    let response = market.exec_update_position_collateral_impact_size(
        &trader,
        position_id,
        fail_max_param_collateral.parse().unwrap(),
        None,
    );
    assert!(
        response.is_err(),
        "Bumping beyond max param results in failure"
    );
}
