//! PERP-808

use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::prelude::*;

// Before counterleverage fixes, this test case would fail for collateral-is-quote.
#[test]
fn counterleverage_too_low() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            "5",
            DirectionToBase::Long,
            None,
            None,
            TakeProfitTrader::Finite("1000000000".parse().unwrap()),
        )
        .unwrap();
}

// This never reproduced the bug, likely because the contracts already prevented it.
#[test]
fn counterleverage_too_high() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position_take_profit(
            &trader,
            "10",
            "5",
            DirectionToBase::Long,
            None,
            None,
            TakeProfitTrader::Finite("1.0000000000001".parse().unwrap()),
        )
        .unwrap();
    let _pos = market.query_position(pos_id);
}
