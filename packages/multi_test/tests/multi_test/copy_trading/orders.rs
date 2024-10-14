use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::copy_trading::{DecQueuePositionId, QueuePositionId, WorkDescription, WorkResp},
    storage::DirectionToBase,
};

use crate::copy_trading::{deposit_money, load_markets};

#[test]
fn place_order() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);

    deposit_money(&market, &trader, "200").unwrap();
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader queues to open a limit order
    market
        .exec_copy_trading_place_order("50", "0.9", DirectionToBase::Long, "1.5")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::ProcessQueueItem {
                id: QueuePositionId::DecQueuePositionId(DecQueuePositionId::new(0))
            }
        }
    );

    // Process queue item: place limit order
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let order_ids = market
        .query_limit_orders(&market.copy_trading_addr, None, None, None)
        .unwrap()
        .orders;

    assert!(order_ids.len() == 1);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "150".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    // We don't know if we were able to successfully finish
    assert!(items.items.iter().any(|item| item.status.pending()));

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::HandleDeferredExecId {}
        }
    );

    // Process queue item: Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // No change in available collateral
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "150".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();
    // We don't know if we were able to successfully finish
    assert!(items.items.iter().all(|item| item.status.finish()));
}

#[test]
#[ignore]
fn place_order_fail() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);

    deposit_money(&market, &trader, "200").unwrap();
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader queues to open a limit order
    market
        .exec_copy_trading_place_order("0.1", "0.8", DirectionToBase::Long, "1.2")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::ProcessQueueItem {
                id: QueuePositionId::DecQueuePositionId(DecQueuePositionId::new(0))
            }
        }
    );

    // Process queue item: place limit order
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let order_ids = market
        .query_limit_orders(&market.copy_trading_addr, None, None, None)
        .unwrap()
        .orders;

    assert!(order_ids.is_empty());

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();

    assert!(items.items.iter().any(|item| item.status.failed()));

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());
}
