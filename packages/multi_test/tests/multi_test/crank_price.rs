use levana_perpswap_multi_test::{
    config::{SpotPriceKind, DEFAULT_MARKET},
    market_wrapper::PerpsMarket,
    response::CosmosResponseExt,
    PerpsApp,
};
use perpswap::storage::DirectionToBase;

#[test]
fn crank_price_2854() {
    let market = PerpsMarket::new_with_type(
        PerpsApp::new_cell().unwrap(),
        DEFAULT_MARKET.collateral_type,
        true,
        SpotPriceKind::Oracle,
    )
    .unwrap();

    let trader = market.clone_trader(0).unwrap();

    // create a position so there will be some work to do, i.e. liquifunding
    market
        .exec_open_position_queue_only(
            &trader,
            "100",
            "9",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let cranker = market.clone_trader(1).unwrap();

    // 1 item in the queue
    assert_eq!(market.query_status().unwrap().deferred_execution_items, 1);

    // crank as much as we want...
    market.exec_crank_n(&cranker, 10).unwrap();
    market.exec_crank_n(&cranker, 10).unwrap();

    // still 1 item in the queue
    assert_eq!(market.query_status().unwrap().deferred_execution_items, 1);

    // ah but if we add a new price...
    market.exec_refresh_price().unwrap();

    // well, there's still 1 item in the queue, because we only "cranked" with 0 execs (i.e. just spot price append)
    assert_eq!(market.query_status().unwrap().deferred_execution_items, 1);

    // gotta crank it
    let res = market.exec_crank(&cranker).unwrap();

    // now the queue is empty
    assert_eq!(market.query_status().unwrap().deferred_execution_items, 0);

    // and the position was opened in that crank exec
    let pos_id = res.event_first_value("position-open", "pos-id").unwrap();
    assert_eq!(pos_id, "1");
}
