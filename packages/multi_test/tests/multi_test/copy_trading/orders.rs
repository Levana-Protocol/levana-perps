use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::copy_trading::{
        self, DecQueuePositionId, QueuePositionId, WorkDescription, WorkResp,
    },
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
fn order_to_position_does_not_produce_deferred_exec() {
    let perps = PerpsApp::new_cell().unwrap();
    let market = PerpsMarket::new(perps.clone()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let copy_trading_addr = market.copy_trading_addr.clone();

    load_markets(&market);

    deposit_money(&market, &trader, "200").unwrap();
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader queues to open a limit order
    market
        .exec_copy_trading_place_order("10", "0.8", DirectionToBase::Long, "1.9")
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

    let deferred_execs = market.query_deferred_execs(&copy_trading_addr).unwrap();
    assert!(!deferred_execs.is_empty());
    assert!(deferred_execs.iter().any(|item| item.status.is_pending()));
    // We have no work since the deferred exec id is not yet executed
    // and is in a pending status
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    market.exec_crank_till_finished(&lp).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    let deferred_execs = market.query_deferred_execs(&copy_trading_addr).unwrap();
    // Now it's not pending anymore
    assert!(!deferred_execs.iter().any(|item| item.status.is_pending()));
    // Do deferred work
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let initial_order_ids = market
        .query_limit_orders(&market.copy_trading_addr, None, None, None)
        .unwrap()
        .orders;

    assert!(!initial_order_ids.is_empty());

    // Set price so that order price is triggered
    market.exec_set_price("0.8".try_into().unwrap()).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let order_ids = market
        .query_limit_orders(&market.copy_trading_addr, None, None, None)
        .unwrap()
        .orders;

    // No more orders are present
    assert!(order_ids.is_empty());

    let positions = market.query_positions(&copy_trading_addr).unwrap();
    // We have one position now
    assert_eq!(positions.len(), 1);

    let final_deferred_execs = market.query_deferred_execs(&copy_trading_addr).unwrap();
    // There are no new deferred exec ids once the new position has opened
    assert_eq!(deferred_execs, final_deferred_execs);
}

#[test]
fn place_order_fail() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    // Set some crank fee configration so that crank fee logic is
    // kicked in
    market
        .exec_set_config(perpswap::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    load_markets(&market);

    deposit_money(&market, &trader, "200").unwrap();
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader queues to open a limit order
    market
        .exec_copy_trading_place_order("0.0001", "0.8", DirectionToBase::Long, "1.2")
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

#[test]
fn cancel_order() {
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

    // Now let's cancel the order!
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(perpswap::storage::MarketExecuteMsg::CancelLimitOrder {
                order_id: order_ids[0].order_id,
            }),
            collateral: None,
        })
        .unwrap();
    // Process queue item: cancel limit order
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let order_ids = market
        .query_limit_orders(&market.copy_trading_addr, None, None, None)
        .unwrap()
        .orders;

    assert!(order_ids.is_empty());

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Let's do deposit, to rebalance the increase of money
    market
        .exec_copytrading_mint_and_deposit(&trader, "3")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_rebalance());
    // Rebalance work
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    // Compute lp token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // deposit
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "203".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();

    assert!(items.items.iter().all(|item| item.status.finish()));
}

#[test]
fn cancel_order_failure() {
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

    // Non existent order id
    let order_id = "100".parse().unwrap();
    assert_ne!(order_ids[0].order_id, order_id);
    // Now let's cancel the order!
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(perpswap::storage::MarketExecuteMsg::CancelLimitOrder { order_id }),
            collateral: None,
        })
        .unwrap();
    // Process queue item: cancel limit order
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let order_ids = market
        .query_limit_orders(&market.copy_trading_addr, None, None, None)
        .unwrap()
        .orders;

    // Because it was not able to cancel the order id
    assert!(order_ids.len() == 1);

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "150".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();

    assert!(items.items.iter().any(|item| item.status.failed()));
}
