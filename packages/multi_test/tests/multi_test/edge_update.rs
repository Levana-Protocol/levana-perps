use std::panic::catch_unwind;

use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::prelude::*;

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

    // Max value of max gains where update is possible
    let high_max_gains = match market_type {
        MarketType::CollateralIsQuote => "5",
        MarketType::CollateralIsBase => {
            "9999999999999999999999999999999999999999999999999999999999"
        }
    };

    market
        .exec_update_position_max_gains(&trader, position_id, high_max_gains.parse().unwrap())
        .unwrap();

    // Updating to this value will result in failure
    let fail_max_gains = match market_type {
        MarketType::CollateralIsQuote => "6",
        MarketType::CollateralIsBase => {
            "99999999999999999999999999999999999999999999999999999999990000"
        }
    };

    match market_type {
        MarketType::CollateralIsQuote => {
            let response = market.exec_update_position_max_gains(
                &trader,
                position_id,
                fail_max_gains.parse().unwrap(),
            );
            assert!(response.is_err())
        }
        MarketType::CollateralIsBase => {
            // Going beyond the max value results in panic because of unwrap in ops.rs
            let response = catch_unwind(|| {
                let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
                market.exec_update_position_max_gains(
                    &trader,
                    position_id,
                    fail_max_gains.parse().unwrap(),
                )
            });
            assert!(response.is_err());
        }
    }

    // Lowest value of max gains where it can be updated
    let low_max_gains = "0.2";

    market
        .exec_update_position_max_gains(&trader, position_id, low_max_gains.parse().unwrap())
        .unwrap();

    // Going below lowest value will result in failure
    let fail_max_gains = "0.1";

    let response = market.exec_update_position_max_gains(
        &trader,
        position_id,
        fail_max_gains.parse().unwrap(),
    );

    assert!(response.is_err())
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
        MarketType::CollateralIsBase => "99",
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
