use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, return_unless_market_collateral_base, time::TimeJump, PerpsApp,
};
use msg::prelude::*;

#[test]
fn test_infinite_max_gains_fail() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_base!(market);

    let trader = market.clone_trader(0).unwrap();

    // This fails because infinite max gains is only allowed on a long position
    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "+Inf",
            None,
            None,
            None,
        )
        .unwrap_err();

    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "+Inf",
            None,
            None,
            None,
        )
        .unwrap();
}

#[test]
fn infinite_max_gains_perp_481() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    // Only works in collateral-is-base markets, since otherwise we cannot have infinite max gains.
    return_unless_market_collateral_base!(market);
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&Addr::unchecked("provider"), 1_000_000_000u64.into())
        .unwrap();

    const ORIG_PRICE: u64 = 100;
    market
        .exec_set_price(ORIG_PRICE.to_string().parse().unwrap())
        .unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "0.111",
            "20",
            DirectionToBase::Long,
            "+Inf",
            None,
            None,
            None,
        )
        .unwrap();

    for i in 0..50 {
        // Jump time, make sure the position either liquidated from borrow fee
        // payments or is still infinite.
        market.set_time(TimeJump::Liquifundings(1)).unwrap();
        // Play around with moving the price to cause active collateral to keep
        // changing.
        market
            .exec_set_price(
                (match i % 4 {
                    0 => 101,
                    1 => 99,
                    2 => 104,
                    _ => 98,
                })
                .to_string()
                .parse()
                .unwrap(),
            )
            .unwrap();
        market
            .exec_crank_till_finished(&Addr::unchecked("cranker"))
            .unwrap();

        // If the position closed (from liquidation), our tests are done.
        if market.query_closed_position(&trader, pos_id).is_ok() {
            return;
        }

        // Position is still open, confirm that it's still considered infinite.

        let res = market.query_position(pos_id).unwrap();
        // assert_eq!(
        //     res.max_gains_in_quote,
        //     MaxGainsInQuote::PosInfinity,
        //     "Max gains is not infinite on iteration {i}, actual: {}",
        //     res.max_gains_in_quote
        // );
        assert_eq!(
            res.take_profit_override, None,
            "Take profit price override is not infinite on iteration {i}, actual: {:?}",
            res.take_profit_override
        );
        assert_eq!(
            res.take_profit_price_base, None,
            "Take profit price is not infinite on iteration {i}, actual: {:?}",
            res.take_profit_price_base
        );
    }
}

#[test]
fn infinite_gains_are_infinite_perp_898() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    // Only works in collateral-is-base markets, since otherwise we can have infinite max gains.
    return_unless_market_collateral_base!(market);
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "20",
            DirectionToBase::Long,
            "+Inf",
            None,
            None,
            None,
        )
        .unwrap();

    // Move the price ridiculously high, we're still not take profitted
    market
        .exec_set_price("1000000000".parse().unwrap())
        .unwrap();
    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    // Confirm the position is still open
    market.query_position(pos_id).unwrap();
}

#[test]
fn update_stop_loss_inf_max_gains_perp_1071() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    // Only works in collateral-is-base markets, since otherwise we can have infinite max gains.
    return_unless_market_collateral_base!(market);
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "20",
            DirectionToBase::Long,
            "+Inf",
            None,
            None,
            None,
        )
        .unwrap();

    // Open a larger short position and run a few liquifundings so that our
    // position ends up making some funding fees. Now the calculated take profit
    // should be negative.
    market
        .exec_open_position(
            &trader,
            "300",
            "30",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    market.set_time(TimeJump::Liquifundings(3)).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    market.exec_set_price("2.0".parse().unwrap()).unwrap();

    market
        .exec_set_trigger_order(&trader, pos_id, Some("0.99".parse().unwrap()), None)
        .unwrap();
}
