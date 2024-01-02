use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp, config::{DEFAULT_MARKET, SpotPriceKind}};
use msg::prelude::*;

#[test]
fn oracle_open_position() {
    let market = PerpsMarket::new_with_type(PerpsApp::new_cell().unwrap(),DEFAULT_MARKET.collateral_type, true, SpotPriceKind::Oracle).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let queue_res = market
        .exec_open_position_queue_only(&trader, "100", "9", DirectionToBase::Long, "1.0", None, None, None)
        .unwrap();

    market.exec_refresh_price().unwrap();

    let (pos_id, _) = market.exec_open_position_process_queue_response(&trader, queue_res).unwrap();
    println!("{}", pos_id);
}
