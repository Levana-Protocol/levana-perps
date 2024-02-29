use levana_perpswap_multi_test::config::{DefaultMarket, DEFAULT_MARKET};
use levana_perpswap_multi_test::time::TimeJump;
use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, position_helpers::assert_position_liquidated,
    response::CosmosResponseExt, PerpsApp,
};
use msg::contracts::market::entry::StatusResp;
use msg::contracts::market::{
    config::ConfigUpdate, entry::QueryMsg, position::events::PositionUpdateEvent,
};
use msg::prelude::*;

#[test]
fn position_update_collateral_impact_leverage() {
    let perform_update = |direction: DirectionToBase,
                          collateral_delta: Signed<Collateral>,
                          expected_leverage: Number| {
        let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
        market
            .exec_set_config(ConfigUpdate {
                borrow_fee_rate_max_annualized: Some("0.00000000000000001".parse().unwrap()),
                borrow_fee_rate_min_annualized: Some("0.00000000000000001".parse().unwrap()),
                ..Default::default()
            })
            .unwrap();

        let trader = market.clone_trader(0).unwrap();
        let initial_collateral = Collateral::from_str("100").unwrap();

        let (pos_id, _) = market
            .exec_open_position(
                &trader,
                initial_collateral.into_number(),
                "10",
                direction,
                "1.0",
                None,
                None,
                None,
            )
            .unwrap();

        let pos = market.query_position(pos_id).unwrap();

        market
            .exec_update_position_collateral_impact_leverage(&trader, pos_id, collateral_delta)
            .unwrap();

        let updated_pos = market.query_position(pos_id).unwrap();

        assert_eq!(
            updated_pos.deposit_collateral,
            initial_collateral.into_signed() + collateral_delta
        );

        assert!(
            updated_pos
                .leverage
                .into_number()
                .approx_eq_eps(expected_leverage.into_number(), Number::EPS_E6),
            "direction: {:?}, expected_leverage: {}, actual_leverage: {}",
            direction,
            expected_leverage,
            updated_pos.leverage
        );

        assert_eq!(updated_pos.notional_size, pos.notional_size);
    };

    // Test add collateral

    let collateral_delta = Signed::<Collateral>::from_str("50").unwrap();
    match DefaultMarket::market_type() {
        MarketType::CollateralIsQuote => {
            perform_update(
                DirectionToBase::Long,
                collateral_delta,
                "6.691648".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                collateral_delta,
                "6.691648".parse().unwrap(),
            );
        }
        MarketType::CollateralIsBase => {
            perform_update(
                DirectionToBase::Long,
                collateral_delta,
                "7.020026".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                collateral_delta,
                "6.363928".parse().unwrap(),
            );
        }
    }

    // Test remove collateral

    let collateral_delta = Signed::<Collateral>::from_str("-50").unwrap();
    match DefaultMarket::market_type() {
        MarketType::CollateralIsQuote => {
            perform_update(
                DirectionToBase::Long,
                collateral_delta,
                "20.226537".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                collateral_delta,
                "20.226537".parse().unwrap(),
            );
        }
        MarketType::CollateralIsBase => {
            perform_update(
                DirectionToBase::Long,
                collateral_delta,
                "19.181454".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                collateral_delta,
                "21.277674".parse().unwrap(),
            );
        }
    }
}

#[test]
fn position_update_collateral_impact_size() {
    let perform_update = |direction: DirectionToBase,
                          collateral_delta: Signed<Collateral>,
                          expected_size: Number| {
        let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
        market
            .exec_set_config(ConfigUpdate {
                ..Default::default()
            })
            .unwrap();
        let trader = market.clone_trader(0).unwrap();
        let initial_collateral = Collateral::from_str("100").unwrap();

        let (pos_id, _) = market
            .exec_open_position(
                &trader,
                initial_collateral.into_number(),
                "10",
                direction,
                "1.0",
                None,
                None,
                None,
            )
            .unwrap();

        market.set_time(TimeJump::Blocks(-1)).unwrap();

        market
            .exec_update_position_collateral_impact_size(&trader, pos_id, collateral_delta, None)
            .unwrap();

        market.set_time(TimeJump::Blocks(-1)).unwrap();
        let updated_pos = market.query_position(pos_id).unwrap();

        assert_eq!(
            updated_pos.deposit_collateral,
            initial_collateral.into_signed() + collateral_delta
        );

        assert!(
            updated_pos
                .notional_size
                .into_number()
                .approx_eq_eps(expected_size.into_number(), Number::EPS_E6),
            "direction: {:?}, expected_size: {}, actual_size: {}",
            direction,
            expected_size,
            updated_pos.notional_size
        );
    };

    // Test add collateral

    let collateral_delta = Signed::<Collateral>::from_str("50").unwrap();
    match DefaultMarket::market_type() {
        MarketType::CollateralIsQuote => {
            perform_update(
                DirectionToBase::Long,
                collateral_delta,
                "1502.815769".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                collateral_delta,
                "-1502.815769".parse().unwrap(),
            );
        }
        MarketType::CollateralIsBase => {
            perform_update(
                DirectionToBase::Long,
                collateral_delta,
                "-1352.256803".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                collateral_delta,
                "1653.449158".parse().unwrap(),
            );
        }
    }

    // Test remove collateral

    let collateral_delta = Signed::<Collateral>::from_str("-50").unwrap();
    match DefaultMarket::market_type() {
        MarketType::CollateralIsQuote => {
            perform_update(
                DirectionToBase::Long,
                collateral_delta,
                "497.184231".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                collateral_delta,
                "-497.184231".parse().unwrap(),
            );
        }
        MarketType::CollateralIsBase => {
            perform_update(
                DirectionToBase::Long,
                collateral_delta,
                "-447.743196".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                collateral_delta,
                "546.550841".parse().unwrap(),
            );
        }
    }
}

#[test]
fn position_update_max_gains() {
    let perform_update = |direction: DirectionToBase,
                          max_gains: MaxGainsInQuote,
                          expected_counter_collateral: Number| {
        let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
        let trader = market.clone_trader(0).unwrap();
        let initial_collateral = Collateral::from_str("100").unwrap();

        let (pos_id, _) = market
            .exec_open_position(
                &trader,
                initial_collateral.into_number(),
                "10",
                direction,
                "3.0",
                None,
                None,
                None,
            )
            .unwrap();

        market.set_time(TimeJump::Blocks(-1)).unwrap();

        market
            .exec_update_position_max_gains(&trader, pos_id, max_gains)
            .unwrap();

        market.set_time(TimeJump::Blocks(-1)).unwrap();
        let updated_pos = market.query_position(pos_id).unwrap();

        assert!(
            updated_pos
                .counter_collateral
                .into_number()
                .approx_eq_eps(expected_counter_collateral.into_number(), Number::EPS_E6),
            "direction: {:?}, expected_counter_collateral: {}, actual_counter_collateral: {}",
            direction,
            expected_counter_collateral,
            updated_pos.counter_collateral
        );
    };

    // Test increase max gains

    let max_gains = MaxGainsInQuote::from_str("5").unwrap();
    match DefaultMarket::market_type() {
        MarketType::CollateralIsQuote => {
            perform_update(
                DirectionToBase::Long,
                max_gains,
                "496.699996670471841705".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                max_gains,
                "496.699996670471841705".parse().unwrap(),
            );
        }
        MarketType::CollateralIsBase => {
            perform_update(
                DirectionToBase::Long,
                max_gains,
                "298.986218".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                max_gains,
                "1080.875958405433354248".parse().unwrap(),
            );
        }
    }

    // Test decrease max gains

    let max_gains = MaxGainsInQuote::from_str("2").unwrap();
    match DefaultMarket::market_type() {
        MarketType::CollateralIsQuote => {
            perform_update(
                DirectionToBase::Long,
                max_gains,
                "198.679998668188736682".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                max_gains,
                "198.679998668188736682".parse().unwrap(),
            );
        }
        MarketType::CollateralIsBase => {
            perform_update(
                DirectionToBase::Long,
                max_gains,
                "149.366921".parse().unwrap(),
            );
            perform_update(
                DirectionToBase::Short,
                max_gains,
                "271.992259356357591301".parse().unwrap(),
            );
        }
    }
}

fn position_update_open_interest_inner(
    market: PerpsMarket,
    direction: DirectionToBase,
    expected_notional_size: Signed<Notional>,
    expected_notional_size_updated: Signed<Notional>,
) {
    let query_notional_interest = || -> (Notional, Notional) {
        let open_interest: StatusResp = market.query(&QueryMsg::Status { price: None }).unwrap();
        match direction {
            DirectionToBase::Long => (open_interest.long_notional, open_interest.short_notional),
            DirectionToBase::Short => (open_interest.short_notional, open_interest.long_notional),
        }
    };

    market
        .exec_set_config(ConfigUpdate {
            // very small value to minimize the borrow fee impact
            borrow_fee_rate_max_annualized: Some("0.00000000000000001".parse().unwrap()),
            borrow_fee_rate_min_annualized: Some("0.00000000000000001".parse().unwrap()),
            trading_fee_notional_size: Some("0.001".parse().unwrap()),
            trading_fee_counter_collateral: Some("0.001".parse().unwrap()),
            delta_neutrality_fee_sensitivity: Some("50000000".parse().unwrap()),
            delta_neutrality_fee_cap: Some("0.01".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(&trader, "100", "9", direction, "1.0", None, None, None)
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();
    let (notional_size_to_check, other_notional_size) = query_notional_interest();

    assert_eq!(
        expected_notional_size.abs(),
        notional_size_to_check.into_signed()
    );
    assert_eq!(Notional::zero(), other_notional_size);
    assert_eq!(pos.notional_size, expected_notional_size);

    let _ = market
        .exec_update_position_leverage(&trader, pos.id, "20".try_into().unwrap(), None)
        .unwrap();

    let updated_pos = market.query_position(pos.id).unwrap();
    let (notional_size_to_check, other_notional_size) = query_notional_interest();

    assert!(expected_notional_size_updated
        .abs()
        .into_number()
        .approx_eq(notional_size_to_check.into_number()));
    assert_eq!(Notional::zero(), other_notional_size);
    assert!(updated_pos
        .notional_size
        .into_number()
        .approx_eq(expected_notional_size_updated.into_number()));

    market.exec_close_position(&trader, pos_id, None).unwrap();
    let _pos = market.query_closed_position(&trader, pos.id).unwrap();
    let open_interest: StatusResp = market.query(&QueryMsg::Status { price: None }).unwrap();

    assert_eq!(open_interest.short_notional, Notional::zero());
    assert_eq!(open_interest.long_notional, Notional::zero());
}

#[test]
fn position_update_short_open_interest() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let expected_notional_size = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => Signed::<Notional>::from_str("-900"),
        MarketType::CollateralIsBase => Signed::<Notional>::from_str("1000"),
    }
    .unwrap();

    let expected_notional_size_updated = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => Signed::<Notional>::from_str("-1979.838"),
        MarketType::CollateralIsBase => Signed::<Notional>::from_str("2076.165"),
    }
    .unwrap();

    position_update_open_interest_inner(
        market,
        DirectionToBase::Short,
        expected_notional_size,
        expected_notional_size_updated,
    )
}

#[test]
fn position_update_long_open_interest() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let expected_notional_size = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => Signed::<Notional>::from_str("900"),
        MarketType::CollateralIsBase => Signed::<Notional>::from_str("-800"),
    }
    .unwrap();

    let expected_notional_size_updated = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => Signed::<Notional>::from_str("1979.838"),
        MarketType::CollateralIsBase => Signed::<Notional>::from_str("-1883.1584"),
    }
    .unwrap();

    position_update_open_interest_inner(
        market,
        DirectionToBase::Long,
        expected_notional_size,
        expected_notional_size_updated,
    )
}

#[test]
fn position_update_negative_collateral_fail() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let perform_update = |direction: DirectionToBase| {
        let (pos_id, _) = market
            .exec_open_position(&trader, "100", "9", direction, "1", None, None, None)
            .unwrap();

        let pos = market.query_position(pos_id).unwrap();
        let collateral_delta = Signed::<Collateral>::from_str("-99").unwrap();

        market
            .exec_update_position_collateral_impact_leverage(&trader, pos_id, collateral_delta)
            .unwrap_err();

        let updated_pos = market.query_position(pos.id).unwrap();

        assert_eq!(
            updated_pos.deposit_collateral, pos.deposit_collateral,
            "test failed for {:?}",
            direction
        );
    };

    perform_update(DirectionToBase::Long);
    perform_update(DirectionToBase::Short);
}

#[test]
fn position_update_leverage() {
    let perform_update = |market: PerpsMarket,
                          direction: DirectionToBase,
                          expected_active_collateral: NonZero<Collateral>,
                          expected_leverage: LeverageToBase| {
        let trader = market.clone_trader(0).unwrap();

        market
            .exec_set_config(ConfigUpdate {
                minimum_deposit_usd: Some("0".parse().unwrap()),
                trading_fee_notional_size: Some("0.0005".parse().unwrap()),
                trading_fee_counter_collateral: Some("0.0005".parse().unwrap()),
                delta_neutrality_fee_sensitivity: Some("50000000".parse().unwrap()),
                delta_neutrality_fee_cap: Some("0.01".parse().unwrap()),
                ..Default::default()
            })
            .unwrap();

        let (pos_id, _) = market
            .exec_open_position(&trader, "100", "9", direction, "1", None, None, None)
            .unwrap();
        let pos = market.query_position(pos_id).unwrap();

        market
            .exec_update_position_leverage(&trader, pos.id, "20".try_into().unwrap(), None)
            .unwrap();

        let updated_pos = market.query_position(pos.id).unwrap();

        assert_eq!(updated_pos.deposit_collateral, pos.deposit_collateral);
        assert!(
            updated_pos
                .active_collateral
                .into_number()
                .approx_eq_eps(expected_active_collateral.into_number(), Number::EPS_E6),
            "active_collateral {} is not equal to expected {} for {:?} position",
            updated_pos.active_collateral,
            expected_active_collateral,
            direction
        );
        assert!(
            updated_pos
                .leverage
                .into_number()
                .approx_eq_eps(expected_leverage.into_number(), Number::EPS_E6),
            "leverage {} is not equal to expected {} for {:?} position",
            updated_pos.leverage,
            expected_leverage,
            direction
        );
    };

    // Test long position

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let expected_leverage = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => LeverageToBase::from_str("20.128867").unwrap(),
        MarketType::CollateralIsBase => LeverageToBase::from_str("20.120947").unwrap(),
    };

    let expected_active_collateral = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => NonZero::<Collateral>::from_str("98.854939").unwrap(),
        MarketType::CollateralIsBase => NonZero::<Collateral>::from_str("98.923886").unwrap(),
    };

    perform_update(
        market,
        DirectionToBase::Long,
        expected_active_collateral,
        expected_leverage,
    );

    // Test short position

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let expected_leverage = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => LeverageToBase::from_str("20.128867").unwrap(),
        MarketType::CollateralIsBase => LeverageToBase::from_str("20.137244").unwrap(),
    };

    let expected_active_collateral = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => NonZero::<Collateral>::from_str("98.854939").unwrap(),
        MarketType::CollateralIsBase => NonZero::<Collateral>::from_str("98.781916").unwrap(),
    };

    perform_update(
        market,
        DirectionToBase::Short,
        expected_active_collateral,
        expected_leverage,
    );
}

#[test]
fn position_update_collateral() {}

#[test]
fn test_position_update_max_leverage_fail() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "5",
            DirectionToBase::Short,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    let balance_before_update = market.query_collateral_balance(&trader).unwrap();

    market
        .exec_update_position_leverage(
            &trader,
            pos_id,
            (market.query_config().unwrap().max_leverage * Number::from(2u64))
                .to_string()
                .parse()
                .unwrap(),
            None,
        )
        .unwrap_err();

    let balance_after_update = market.query_collateral_balance(&trader).unwrap();
    assert_eq!(balance_before_update, balance_after_update);
}

#[test]
fn position_update_abs_notional_size() {
    #[derive(Clone, Copy, Debug)]
    enum ChangeKind {
        Collateral(Number),
        Leverage(NumberGtZero),
    }

    #[derive(Clone, Copy, Debug)]
    enum Sign {
        Positive,
        Negative,
    }

    impl Sign {
        fn invert(self) -> Self {
            match self {
                Self::Positive => Self::Negative,
                Self::Negative => Self::Positive,
            }
        }
    }

    impl From<Number> for Sign {
        fn from(src: Number) -> Self {
            if src.is_negative() {
                Self::Negative
            } else {
                Self::Positive
            }
        }
    }

    impl ChangeKind {
        pub fn size_sign_abs(&self) -> Sign {
            match *self {
                ChangeKind::Collateral(delta) => delta.into(),
                ChangeKind::Leverage(leverage) => {
                    (Number::from(leverage) - Number::from(10u64)).into()
                }
            }
        }
        pub fn size_sign_opinionated(
            &self,
            direction: DirectionToBase,
            market_type: MarketType,
        ) -> Sign {
            let abs_sign: Sign = self.size_sign_abs();

            match (market_type, direction) {
                (MarketType::CollateralIsQuote, DirectionToBase::Long) => abs_sign,
                (MarketType::CollateralIsQuote, DirectionToBase::Short) => abs_sign.invert(),
                (MarketType::CollateralIsBase, DirectionToBase::Long) => abs_sign.invert(),
                (MarketType::CollateralIsBase, DirectionToBase::Short) => abs_sign,
            }
        }
    }

    let initial_collateral: Number = 100u64.into();
    let initial_leverage: Number = 10u64.into();
    let max_gains: Number = 1u64.into();

    let market_types = vec![MarketType::CollateralIsQuote, MarketType::CollateralIsBase];
    let directions = vec![DirectionToBase::Long, DirectionToBase::Short];
    let change_kinds = vec![
        ChangeKind::Collateral(Number::try_from("10").unwrap()),
        ChangeKind::Collateral(Number::try_from("-10").unwrap()),
        ChangeKind::Leverage(NumberGtZero::try_from("15").unwrap()),
        ChangeKind::Leverage(NumberGtZero::try_from("5").unwrap()),
    ];

    for market_type in market_types {
        for direction in directions.clone() {
            for change_kind in change_kinds.clone() {
                let market = PerpsMarket::new_with_type(
                    PerpsApp::new_cell().unwrap(),
                    market_type,
                    true,
                    DEFAULT_MARKET.spot_price,
                )
                .unwrap();
                let trader = market.clone_trader(0).unwrap();

                // with a price of just 1 it's hard to see the changes between market types, so change it
                market.exec_set_price("2".try_into().unwrap()).unwrap();

                let (pos_id, _) = market
                    .exec_open_position(
                        &trader,
                        initial_collateral,
                        initial_leverage.to_string().as_str(),
                        direction,
                        max_gains.to_string().as_str(),
                        None,
                        None,
                        None,
                    )
                    .unwrap();

                let original_pos = market.query_position(pos_id).unwrap();

                let update_res = match change_kind {
                    ChangeKind::Collateral(delta) => market
                        .exec_update_position_collateral_impact_size(
                            &trader,
                            pos_id,
                            Signed::<Collateral>::from_number(delta),
                            None,
                        )
                        .unwrap(),
                    ChangeKind::Leverage(leverage) => market
                        .exec_update_position_leverage(&trader, pos_id, leverage.into(), None)
                        .unwrap(),
                };

                let evt: PositionUpdateEvent = update_res
                    .exec_resp()
                    .event_first("position-update")
                    .unwrap()
                    .try_into()
                    .unwrap();

                let updated_pos = market.query_position(pos_id).unwrap();

                assert_eq!(
                    updated_pos.notional_size - original_pos.notional_size,
                    evt.notional_size_delta
                );

                match change_kind.size_sign_abs() {
                    Sign::Positive => {
                        assert!(evt.notional_size_abs_delta.is_strictly_positive());
                    }
                    Sign::Negative => {
                        assert!(evt.notional_size_abs_delta.is_negative());
                    }
                }

                match change_kind.size_sign_opinionated(direction, market_type) {
                    Sign::Positive => {
                        assert!(evt.notional_size_delta.is_strictly_positive());
                    }
                    Sign::Negative => {
                        assert!(evt.notional_size_delta.is_negative());
                    }
                }
            }
        }
    }
}

#[test]
fn leverage_over_30() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market.exec_set_price("100".parse().unwrap()).unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "10",
            "30",
            DirectionToBase::Long,
            "10",
            None,
            None,
            None,
        )
        .unwrap();

    // Move the price to force leverage to go over 40
    market.exec_set_price("99".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    let pos = market.query_position(pos_id).unwrap();
    assert!(pos.leverage > "40".parse().unwrap());

    // We can't increase our leverage
    market
        .exec_update_position_leverage(&trader, pos_id, "60".parse().unwrap(), None)
        .unwrap_err();
    market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "-5".parse().unwrap())
        .unwrap_err();

    // We _can_ keep our leverage the same by simply increasing the position size
    market
        .exec_update_position_collateral_impact_size(&trader, pos_id, "5".parse().unwrap(), None)
        .unwrap();

    // Or by decreasing the leverage
    market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "5".parse().unwrap())
        .unwrap();
}

#[test]
fn position_update_max_gains_perp_666() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "4",
            None,
            None,
            None,
        )
        .unwrap();

    let pos1 = market.query_position(pos_id).unwrap();
    market
        .exec_update_position_max_gains(&trader, pos_id, "3".parse().unwrap())
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();

    assert_ne!(pos1.take_profit_price_base, pos.take_profit_price_base);
}

#[test]
fn counter_leverage_less_than_one_perp_778() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            match market.id.get_market_type() {
                MarketType::CollateralIsQuote => DirectionToBase::Long,
                MarketType::CollateralIsBase => DirectionToBase::Short,
            },
            match market.id.get_market_type() {
                MarketType::CollateralIsQuote => "10",
                MarketType::CollateralIsBase => "5",
            },
            None,
            None,
            None,
        )
        .unwrap();

    let price = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => "0.95",
        MarketType::CollateralIsBase => "1.05",
    }
    .parse()
    .unwrap();

    market.exec_set_price(price).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    // Make sure we wrote our test correctly
    let pos = market.query_position(pos_id).unwrap();
    let counter_leverage = pos.counter_leverage.into_number();
    assert!(
        counter_leverage < Number::ONE && counter_leverage > Number::ZERO,
        "Counter leverage is not in range (0, 1): {counter_leverage}"
    );

    // Now try to update the position on something besides max gains
    market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "10".parse().unwrap())
        .unwrap();
}

#[test]
fn update_after_liquidation_fails_perp_873() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            match market.id.get_market_type() {
                MarketType::CollateralIsQuote => DirectionToBase::Long,
                MarketType::CollateralIsBase => DirectionToBase::Short,
            },
            "2",
            None,
            None,
            None,
        )
        .unwrap();

    // Large enough price swing to trigger a liquidation.
    let price = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => "0.2",
        MarketType::CollateralIsBase => "3.5",
    }
    .parse()
    .unwrap();

    market.exec_set_price(price).unwrap();

    // Making an update should fail because we need to be liquidated.
    market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "10".parse().unwrap())
        .unwrap_err();

    // After we crank, the position should be liquidated
    market.exec_crank_till_finished(&trader).unwrap();
    let closed = market.query_closed_position(&trader, pos_id).unwrap();
    assert_position_liquidated(&closed).unwrap();
}

#[test]
fn position_update_collateral_remove_tiny() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

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

    // We can't remove amounts of collateral so tiny that it would require
    // more precision than what the token can express
    let collateral_delta = "-0.000000001".parse().unwrap();

    market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, collateral_delta)
        .unwrap_err();

    market
        .exec_update_position_collateral_impact_size(&trader, pos_id, collateral_delta, None)
        .unwrap_err();
}
