use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::{
        copy_trading::{DecQueuePositionId, QueuePositionId, WorkDescription, WorkResp},
        market::position::PositionId,
    },
    shared::storage::DirectionToBase,
};

use crate::copy_trading::{deposit_money, load_markets, withdraw_money};

#[test]
fn leader_opens_attempt_open_incorrect_position() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);

    deposit_money(&market, &trader, "200");
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader queues to open a position that will fail eventually
    market
        .exec_copy_trading_open_position("2.5", DirectionToBase::Long, "1.5")
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

    // Process queue item: Open the position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap();
    let position_ids = position_ids
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();
    assert!(position_ids.is_empty());

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    // Available collateral hasn't changed
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();
    assert_eq!(items.items.len(), 1);
    // All the items should have been finished
    assert!(!items.items.iter().all(|item| item.status.finish()));

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
}

#[test]
fn leader_opens_correct_position() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);

    deposit_money(&market, &trader, "200");
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader queues to open a new position
    market
        .exec_copy_trading_open_position("50", DirectionToBase::Long, "1.5")
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

    // Process queue item: Open the position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap();
    let position_ids = position_ids
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();
    assert!(position_ids.len() == 1);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "150".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.into())
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
}

#[test]
fn leader_incorrect_position() {
    // Same test as earlier, but also does withdraw initially to check
    // for cases where opening position is not the first element in that queue.
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);

    deposit_money(&market, &trader, "200");
    withdraw_money(&market, &trader, "10");
    withdraw_money(&market, &trader, "10");

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "180".parse().unwrap());

    // Leader queues to open a position that will fail eventually
    market
        .exec_copy_trading_open_position("2.5", DirectionToBase::Long, "1.5")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::ProcessQueueItem {
                id: QueuePositionId::DecQueuePositionId(DecQueuePositionId::new(2))
            }
        }
    );

    // Process queue item: Open the position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap();
    let position_ids = position_ids
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();
    assert!(position_ids.is_empty());

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    // Available collateral hasn't changed
    assert_eq!(tokens[0].collateral, "180".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();
    assert_eq!(items.items.len(), 1);
    // All the items should have been finished
    assert!(!items.items.iter().all(|item| item.status.finish()));

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
}

#[test]
fn leader_open_position_compute_token() {
    // This simulates this scenario: Deposit, withdrawal, Open
    // position and then deposit again. Before the final deposit, LP
    // value computation should happen again.
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "200");
    withdraw_money(&market, &trader, "10");

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "190".parse().unwrap());

    // Leader opens a position
    market
        .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
        .unwrap();

    // Process queue item: Open the position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap();
    let position_ids = position_ids
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();
    assert!(position_ids.len() == 1);

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

    deposit_money(&market, &trader, "50");

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "230".parse().unwrap());
}

#[test]
fn lp_token_value_reduced_after_open() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "200");

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader opens a position
    market
        .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
        .unwrap();

    // Process queue item: Open the position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    // Process queue item: Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let trader1 = market.clone_trader(1).unwrap();

    deposit_money(&market, &trader1, "20");
    let tokens = market.query_copy_trading_balance(&trader1).unwrap();
    let shares = tokens.balance[0].shares;
    // Since token value has reduced, he can buy more shares for the same amount
    assert!(shares.raw() > "20".parse().unwrap());
}

#[test]
fn load_work_after_six_hours() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    load_markets(&market);

    market
        .set_time(levana_perpswap_multi_test::time::TimeJump::Hours(7))
        .unwrap();

    // Process queue item: Load market after 6 hours.
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
}

#[test]
fn leader_position_closed_with_profit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "200");
    withdraw_money(&market, &trader, "10");

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "190".parse().unwrap());

    // Leader opens a position
    market
        .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
        .unwrap();

    // Process queue item: Open the position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    // Process queue item: Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();

    assert_eq!(position_ids.len(), 1);

    withdraw_money(&market, &trader, "10");

    // We are going to make a profit!
    market.exec_set_price("1.5".try_into().unwrap()).unwrap();
    market.exec_crank(&lp).unwrap();

    market
        .query_closed_position(&market.copy_trading_addr, position_ids[0])
        .unwrap();

    market.set_time(levana_perpswap_multi_test::time::TimeJump::Hours(48)).unwrap();
    // Process queue item: Load market after 6 hours.
    market.exec_copytrading_do_work(&trader).unwrap();

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();

    assert_eq!(position_ids.len(), 0);

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let trader1 = market.clone_trader(1).unwrap();
    deposit_money(&market, &trader1, "20");
    // let tokens = market.query_copy_trading_balance(&trader1).unwrap();
    // let shares = tokens.balance[0].shares;
    // println!("shares: {shares}");
    // Since token value has increased, you can buy less shares for the same amount
    // todo: fix it
    // assert!(shares.raw() < "20".parse().unwrap());
}
