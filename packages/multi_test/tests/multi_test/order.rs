use levana_perpswap_multi_test::market_wrapper::PerpsMarket;
use levana_perpswap_multi_test::PerpsApp;
use msg::contracts::market::order::OrderId;
use msg::prelude::*;

#[test]
fn test_place_limit_order_long() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("100".try_into().unwrap()).unwrap();
    market.exec_crank(&cranker).unwrap();

    // Test invalid order: trigger price > spot price

    market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "101".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Long,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap_err();

    // Test invalid order: trigger price < stop loss override

    market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "90".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Long,
            "1".try_into().unwrap(),
            Some("95".try_into().unwrap()),
            None,
        )
        .unwrap_err();

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

    // Test invalid order: trigger price > spot price

    market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "99".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Short,
            "1".try_into().unwrap(),
            None,
            None,
        )
        .unwrap_err();

    // Test invalid order: trigger price < stop loss override

    market
        .exec_place_limit_order(
            &trader,
            "100".try_into().unwrap(),
            "110".try_into().unwrap(),
            "10".try_into().unwrap(),
            DirectionToBase::Short,
            "1".try_into().unwrap(),
            Some("105".try_into().unwrap()),
            None,
        )
        .unwrap_err();

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
    assert_eq!(resp.next_start_after, Some(OrderId(5)));

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
