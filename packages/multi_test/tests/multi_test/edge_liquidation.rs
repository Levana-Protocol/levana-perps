use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, time::TimeJump, PerpsApp};
use msg::{
    contracts::market::config::ConfigUpdate,
    prelude::{DirectionToBase, PriceBaseInQuote},
};

#[test]
fn liquidation_edge() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    // Open a position
    let (position_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "10",
            None,
            None,
            None,
        )
        .unwrap();

    let position_query = market.query_position(position_id).unwrap();

    let liquidation_price = position_query.liquidation_price_base.unwrap();

    let above_liquidation_price = liquidation_price.into_number() + "0.01".parse().unwrap();
    let above_liquidation_price =
        PriceBaseInQuote::try_from_number(above_liquidation_price).unwrap();

    // Set the spot price to slightly above liquidation price
    market.exec_set_price(above_liquidation_price).unwrap();

    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();

    // Ensure that the position doesn't get closed
    market
        .query_closed_position(&trader, position_id)
        .unwrap_err();

    let below_liquidation_price = liquidation_price.into_number() - "0.01".parse().unwrap();
    let below_liquidation_price =
        PriceBaseInQuote::try_from_number(below_liquidation_price).unwrap();

    // Set the spot price below liquidation price
    market.exec_set_price(below_liquidation_price).unwrap();

    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();

    // Ensure that the position gets closed
    market.query_closed_position(&trader, position_id).unwrap();
}

#[test]
fn take_profit_edge() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    // Open a position
    let (position_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "10",
            None,
            None,
            None,
        )
        .unwrap();

    let position_query = market.query_position(position_id).unwrap();

    let take_profit_price = position_query.take_profit_price_trader.unwrap();

    let below_take_profit_price = take_profit_price.into_number() - "0.01".parse().unwrap();
    let below_take_profit_price =
        PriceBaseInQuote::try_from_number(below_take_profit_price).unwrap();

    // Set the spot price to slightly below the take profit price
    market.exec_set_price(below_take_profit_price).unwrap();

    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();

    // Ensure that the position doesn't get closed
    market
        .query_closed_position(&trader, position_id)
        .unwrap_err();

    let above_take_profit_price = take_profit_price.into_number() + "0.01".parse().unwrap();
    let above_take_profit_price =
        PriceBaseInQuote::try_from_number(above_take_profit_price).unwrap();

    // Set the spot price above take profit price
    market.exec_set_price(above_take_profit_price).unwrap();

    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();

    // Ensure that the position gets closed
    market.query_closed_position(&trader, position_id).unwrap();
}

#[test]
fn insufficient_liquidation_margin() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market
        .exec_set_config(ConfigUpdate {
            // The exposure amount changes in response to changes in price.
            // Therefore, if we keep the default higher value for this parameter,
            // we don't end up closing the position after one liquifunding.
            // To account for that, we set a much lower exposure ratio.
            exposure_margin_ratio: Some("0.0001".parse().unwrap()),
            ..ConfigUpdate::default()
        })
        .unwrap();

    let trader = market.clone_trader(0).unwrap();
    // Open a position
    let (position_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "10",
            None,
            None,
            None,
        )
        .unwrap();

    let position_query = market.query_position(position_id).unwrap();

    let liquidation_price = position_query.liquidation_price_base.unwrap();

    let above_liquidation_price =
        liquidation_price.into_number() + "0.000000000000000009".parse().unwrap();
    let above_liquidation_price =
        PriceBaseInQuote::try_from_number(above_liquidation_price).unwrap();

    // Set the spot price to slightly above liquidation price
    market.exec_set_price(above_liquidation_price).unwrap();

    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();

    let position_query = market.query_position(position_id).unwrap();
    assert!(position_query.active_collateral.raw() > position_query.liquidation_margin.total());

    // Trigger liquifunding
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_refresh_price().unwrap();
    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();

    // Ensure that the position got closed
    market.query_closed_position(&trader, position_id).unwrap();
}
