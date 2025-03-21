use levana_perpswap_multi_test::market_wrapper::PerpsMarket;
use levana_perpswap_multi_test::PerpsApp;
use perpswap::contracts::market::config::ConfigUpdate;
use perpswap::contracts::market::entry::{LimitOrderHistoryResp, LimitOrderResult};
use perpswap::contracts::market::order::OrderId;
use perpswap::prelude::*;

#[test]
fn test_place_limit_order_long() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();

    // Test success cases
    // Set two orders to when price moves to exactly trigger price as well as when price moves
    // bellow trigger price

    market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "95".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Long,
            "1".try_into().unwrap(),
            Some("92".try_into().unwrap()),
            None,
        )
        .unwrap();

    market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "94".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Long,
            "1".try_into().unwrap(),
            Some("92".try_into().unwrap()),
            None,
        )
        .unwrap();

    // Drop price to exactly the trigger price of the first order

    market.exec_set_price("95".try_into().unwrap()).unwrap();
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);

    market.exec_crank(&cranker).unwrap();
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 1);

    let resp = market
        .query_limit_orders(&trader, None, None, None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);

    // Drop price to below the trigger price of the second order

    market.exec_set_price("93".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();

    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 2);

    let resp = market
        .query_limit_orders(&trader, None, None, None)
        .unwrap();
    assert_eq!(resp.orders.len(), 0);
}

#[test]
fn test_place_limit_order_short() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();

    // Test success cases
    // Set two orders to when price moves to exactly trigger price as well as when price moves
    // above trigger price

    market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "105".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Short,
            "1".try_into().unwrap(),
            Some("108".try_into().unwrap()),
            None,
        )
        .unwrap();

    market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "106".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Short,
            "1".try_into().unwrap(),
            Some("108".try_into().unwrap()),
            None,
        )
        .unwrap();

    // Increase price to exactly the trigger price of the first order

    market.exec_set_price("105".try_into().unwrap()).unwrap();
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);

    market.exec_crank(&cranker).unwrap();
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 1);

    let resp = market
        .query_limit_orders(&trader, None, None, None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);

    // Increase price to above the trigger price of the second order

    market.exec_set_price("107".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();

    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 2);

    let resp = market
        .query_limit_orders(&trader, None, None, None)
        .unwrap();
    assert_eq!(resp.orders.len(), 0);
}

#[test]
fn test_cancel_limit_order_long() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();

    let (order_id, _) = market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "95".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Long,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap();

    market.query_limit_order(order_id).unwrap();
    market.exec_cancel_limit_order(&trader, order_id).unwrap();
    market.query_limit_order(order_id).unwrap_err();

    market.exec_set_price("95".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn test_cancel_limit_order_short() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();

    let (order_id, _) = market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "105".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Short,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap();

    market.query_limit_order(order_id).unwrap();
    market.exec_cancel_limit_order(&trader, order_id).unwrap();
    market.query_limit_order(order_id).unwrap_err();

    market.exec_set_price("105".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn test_limit_order_query() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    for _ in 0..10 {
        market
            .exec_place_limit_order(
                &trader,
                "100".try_into().unwrap(),
                "105".try_into().unwrap(),
                "10".try_into().unwrap(),
                DirectionToBase::Short,
                "1".try_into().unwrap(),
                None,
                None,
            )
            .unwrap();

        market.exec_crank(&cranker).unwrap();
    }

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 5);

    let order_ids = resp
        .orders
        .iter()
        .map(|order| order.order_id.u64())
        .collect::<Vec<u64>>();
    assert_eq!(order_ids, vec![1, 2, 3, 4, 5]);
    assert_eq!(resp.next_start_after, Some(OrderId::new(5)));

    let resp = market
        .query_limit_orders(&trader, resp.next_start_after, None, None)
        .unwrap();
    assert_eq!(resp.orders.len(), 5);

    let order_ids = resp
        .orders
        .iter()
        .map(|order| order.order_id.u64())
        .collect::<Vec<u64>>();
    assert_eq!(order_ids, vec![6, 7, 8, 9, 10]);
    assert_eq!(resp.next_start_after, None);
}

#[test]
fn failed_order_is_completely_closed_perp_1013() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market
        .exec_place_limit_order(
            &trader,
            // So much collateral that we can't open it because of insufficient liquidity
            "100000".try_into().unwrap(),
            "1.01".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Short,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);

    market.exec_set_price("1.02".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 0);

    // Querying positions should still be safe, and we should get nothing
    let positions = market.query_positions(&trader).unwrap();

    assert_eq!(positions, vec![]);

    // We should see a single failed order in the history
    let LimitOrderHistoryResp {
        mut orders,
        next_start_after,
    } = market.query_limit_order_history(&trader).unwrap();
    assert_eq!(next_start_after, None);
    assert_eq!(orders.len(), 1);
    let order = orders.pop().unwrap();
    match order.result {
        LimitOrderResult::Success { position: _ } => panic!("Should have been a failure"),
        LimitOrderResult::Failure { reason: _ } => (),
    }
}

#[test]
fn limit_order_history_success() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    let LimitOrderHistoryResp {
        orders,
        next_start_after,
    } = market.query_limit_order_history(&trader).unwrap();
    assert_eq!(next_start_after, None);
    assert_eq!(orders.len(), 0);

    market
        .exec_place_limit_order(
            &trader,
            "10".try_into().unwrap(),
            "1.01".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Short,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);
    let LimitOrderHistoryResp {
        orders,
        next_start_after,
    } = market.query_limit_order_history(&trader).unwrap();
    assert_eq!(next_start_after, None);
    assert_eq!(orders.len(), 0);

    market.exec_set_price("1.02".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 0);

    let LimitOrderHistoryResp {
        mut orders,
        next_start_after,
    } = market.query_limit_order_history(&trader).unwrap();
    assert_eq!(next_start_after, None);
    assert_eq!(orders.len(), 1);
    let order = orders.pop().unwrap();
    match order.result {
        LimitOrderResult::Success { position: _ } => (),
        LimitOrderResult::Failure { reason } => panic!("Limit order failed: {reason}"),
    }
}

#[test]
fn failed_order_caps_is_completely_closed_perp_1013() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market
        .exec_set_config(ConfigUpdate {
            delta_neutrality_fee_cap: Some("0.0001".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // Deposit a bunch of liquidity
    market
        .exec_mint_and_deposit_liquidity(&cranker, "1000000000".parse().unwrap())
        .unwrap();

    // First open a bunch of long and short positions, saving the short position IDs
    let mut shorts = vec![];

    for _ in 0..10 {
        market
            .exec_open_position(
                &trader,
                "100",
                "30",
                DirectionToBase::Long,
                "3",
                None,
                None,
                None,
            )
            .unwrap();
        let (pos_id, _) = market
            .exec_open_position(
                &trader,
                "100",
                "30",
                DirectionToBase::Short,
                "3",
                None,
                None,
                None,
            )
            .unwrap();
        shorts.push(pos_id);
    }

    // Close all the shorts so we hit our delta neutrality cap
    for pos_id in shorts {
        market.exec_close_position(&trader, pos_id, None).unwrap();
    }

    // Confirm that we cannot open a new long
    market
        .exec_open_position(
            &trader,
            "100",
            "30",
            DirectionToBase::Long,
            "3",
            None,
            None,
            None,
        )
        .unwrap_err();

    // Now place a long limit order and confirm everything works as expected
    // with the failed position open

    market
        .exec_place_limit_order(
            &trader,
            // So much collateral that we can't open it because of insufficient liquidity
            "10".try_into().unwrap(),
            "0.99".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Long,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);

    market.exec_set_price("0.98".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 0);

    // We should see a single failed order in the history
    let LimitOrderHistoryResp {
        mut orders,
        next_start_after,
    } = market.query_limit_order_history(&trader).unwrap();
    assert_eq!(next_start_after, None);
    assert_eq!(orders.len(), 1);
    let order = orders.pop().unwrap();
    match order.result {
        LimitOrderResult::Success { position: _ } => panic!("Should have been a failure"),
        LimitOrderResult::Failure { reason: _ } => (),
    }
}

#[test]
fn poc_set_other_users_trigger_order_high() {
    // Setup

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();

    // Trader that will execute attack on other trader's positions
    let attacker = market.clone_trader(3).unwrap();

    let take_profit_override = PriceBaseInQuote::try_from_number(105u128.into()).unwrap();

    // Set price of the market to be 1--
    market.exec_set_price("100".try_into().unwrap()).unwrap();
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
    // @audit - Supplied the sender to be the attacker address. The attacker was able to execute a set trigger order for another user.
    let err: PerpError = market
        .exec_update_position_take_profit(
            &attacker,
            pos_id,
            TakeProfitTrader::Finite(take_profit_override.into_non_zero()),
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err.id, ErrorId::Auth);
}

#[test]
fn lagging_crank_perp_1350_long() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // Start off with a price of 10. Place a limit order with a trigger of 7.
    // Then price drops to 6.9, and climbs to 7.1, before any cranking happens.
    //
    // Before deferred execution: We need to guarantee that the position doesn't get opened under those
    // circumstances. Later, when it drops back down to 5, we should enter at 5.
    //
    // Since deferred execution: we want to retroactively use the first price point the triggered,
    // even with a lagging crank.
    market.exec_set_price("10".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    market
        .exec_place_limit_order(
            &trader,
            "10".try_into().unwrap(),
            "7".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Long,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);

    market.exec_set_price("6.9".parse().unwrap()).unwrap();
    market.exec_set_price("7.1".parse().unwrap()).unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);

    market.exec_crank_till_finished(&cranker).unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 0);
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 1);

    // Code below only applied before deferred execution.

    // market.exec_set_price("5".parse().unwrap()).unwrap();
    // market.exec_crank_till_finished(&cranker).unwrap();

    // let resp = market
    //     .query_limit_orders(&trader, None, Some(5u32), None)
    //     .unwrap();
    // assert_eq!(resp.orders.len(), 0);

    // let mut positions = market.query_positions(&trader).unwrap();
    // assert_eq!(positions.len(), 1);
    // let position = positions.pop().unwrap();
    // assert!(position
    //     .entry_price_base
    //     .into_number()
    //     .approx_eq("5".parse().unwrap()));
}

#[test]
fn lagging_crank_perp_1350_short() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // Description below is pre-deferred execution. See test above for explanation
    // of the changes since deferred execution.

    // Start off with a price of 10. Place a limit order with a trigger of 13.
    // Then price climbs to 14, and drops to 12, before any cranking happens. We
    // need to guarantee that the position doesn't get opened under those
    // circumstances. Later, when it climbs back up to 15, we should enter at 15.
    market.exec_set_price("10".parse().unwrap()).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    market
        .exec_place_limit_order(
            &trader,
            "10".try_into().unwrap(),
            "13".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Short,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);

    market.exec_set_price("13.1".parse().unwrap()).unwrap();
    market.exec_set_price("12.9".parse().unwrap()).unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 1);
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);

    market.exec_crank_till_finished(&cranker).unwrap();

    let resp = market
        .query_limit_orders(&trader, None, Some(5u32), None)
        .unwrap();
    assert_eq!(resp.orders.len(), 0);
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 1);

    // market.exec_set_price("15".parse().unwrap()).unwrap();
    // market.exec_crank_till_finished(&cranker).unwrap();

    // let resp = market
    //     .query_limit_orders(&trader, None, Some(5u32), None)
    //     .unwrap();
    // assert_eq!(resp.orders.len(), 0);

    // let mut positions = market.query_positions(&trader).unwrap();
    // assert_eq!(positions.len(), 1);
    // let position = positions.pop().unwrap();
    // assert!(position
    //     .entry_price_base
    //     .into_number()
    //     .approx_eq("15".parse().unwrap()));
}
