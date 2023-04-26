use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, response::CosmosResponseExt, PerpsApp,
};
use msg::{contracts::market::liquidity::events::DeltaNeutralityRatioEvent, prelude::*};

#[test]
fn delta_neutrality_ratio_event() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, res) = market
        .exec_open_position(
            &trader,
            "10",
            "10",
            DirectionToBase::Short,
            "1.5",
            None,
            None,
            None,
        )
        .unwrap();

    DeltaNeutralityRatioEvent::try_from(res.event_first("delta-neutrality-ratio").unwrap())
        .unwrap();

    let res = market
        .exec_deposit_liquidity(&trader, "100".parse().unwrap())
        .unwrap();
    DeltaNeutralityRatioEvent::try_from(res.event_first("delta-neutrality-ratio").unwrap())
        .unwrap();

    let res = market
        .exec_update_position_collateral_impact_size(&trader, pos_id, "50".parse().unwrap(), None)
        .unwrap();
    DeltaNeutralityRatioEvent::try_from(res.event_first("delta-neutrality-ratio").unwrap())
        .unwrap();

    let res = market.exec_reinvest_yield(&trader, None, true).unwrap();
    DeltaNeutralityRatioEvent::try_from(res.event_first("delta-neutrality-ratio").unwrap())
        .unwrap();

    let res = market
        .exec_withdraw_liquidity(&trader, Some("10".parse().unwrap()))
        .unwrap();
    DeltaNeutralityRatioEvent::try_from(res.event_first("delta-neutrality-ratio").unwrap())
        .unwrap();

    let res = market.exec_close_position(&trader, pos_id, None).unwrap();
    DeltaNeutralityRatioEvent::try_from(res.event_first("delta-neutrality-ratio").unwrap())
        .unwrap();
}
