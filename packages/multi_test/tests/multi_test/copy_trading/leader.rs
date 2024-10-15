use cosmwasm_std::{Addr, Event};
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::{
        copy_trading::{DecQueuePositionId, QueuePositionId, WorkDescription, WorkResp},
        market::position::{ClosedPosition, PositionId},
    },
    number::{Collateral, UnsignedDecimal},
    storage::DirectionToBase,
};

use crate::copy_trading::{deposit_money, load_markets, withdraw_money};

#[test]
fn leader_opens_attempt_open_incorrect_position() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);

    deposit_money(&market, &trader, "200").unwrap();
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
        .unwrap()
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

    deposit_money(&market, &trader, "200").unwrap();
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

    deposit_money(&market, &trader, "200").unwrap();
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
    deposit_money(&market, &trader, "200").unwrap();
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

    deposit_money(&market, &trader, "50").unwrap();

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
    let response = deposit_money(&market, &trader, "200").unwrap();
    response.assert_event(&Event::new("wasm-lp-token").add_attribute("value", "1".to_owned()));

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

    let response = deposit_money(&market, &trader1, "20").unwrap();
    let token_event = response
        .events
        .iter()
        .find(|item| item.ty == "wasm-lp-token")
        .unwrap();
    let token_value = token_event
        .attributes
        .iter()
        .find(|item| item.key == "value")
        .unwrap();
    assert!(Collateral::one() > token_value.value.parse().unwrap());

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
    deposit_money(&market, &trader, "200").unwrap();
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
    market.exec_crank_till_finished(&lp).unwrap();

    market
        .query_closed_position(&market.copy_trading_addr, position_ids[0])
        .unwrap();

    let all_position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();

    let closed_position = market
        .query_closed_position(&market.copy_trading_addr, position_ids[0])
        .unwrap();

    assert_eq!(all_position_ids.len(), 0);

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let trader1 = market.clone_trader(1).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader1, "20")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    match work {
        WorkResp::NoWork => panic!("Impossible: No work"),
        WorkResp::HasWork { work_description } => assert!(work_description.is_rebalance()),
    }

    let closed_position_active_collateral = closed_position.active_collateral;
    let leader_comission = closed_position
        .pnl_collateral
        .try_into_non_negative_value()
        .unwrap()
        .checked_mul_dec("0.1".parse().unwrap())
        .unwrap();
    let diff = closed_position_active_collateral
        .checked_sub(leader_comission)
        .unwrap();
    let tokens_collateral: Collateral = "170".parse().unwrap();
    let tokens_collateral = tokens_collateral.checked_add(diff).unwrap();

    // Rebalance the market
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-rebalanced")
        .add_attribute("made-profit", true.to_string())
        .add_attribute("batched", false.to_string());
    response.assert_event(&event);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert!(tokens[0].collateral.diff(tokens_collateral) < "0.1".parse().unwrap());

    // Reset work
    market.exec_copytrading_do_work(&trader1).unwrap();
    // Compute lp token value
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-lp-token").add_attribute("batched", false.to_string());
    response.assert_event(&event);
    let token_event = response
        .events
        .iter()
        .find(|item| item.ty == "wasm-lp-token")
        .unwrap();
    let token_value = token_event
        .attributes
        .iter()
        .find(|item| item.key == "value")
        .unwrap();
    // Token value has increased
    assert!(Collateral::one() < token_value.value.parse().unwrap());
    // Do deposit
    market.exec_copytrading_do_work(&trader1).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert!(tokens[0].unclaimed_commission > Collateral::zero());

    let tokens_collateral = tokens_collateral
        .checked_add("20".parse().unwrap())
        .unwrap();
    assert!(tokens_collateral.diff(tokens[0].collateral) < "0.1".parse().unwrap());

    let tokens = market.query_copy_trading_balance(&trader1).unwrap();
    let shares = tokens.balance[0].shares;
    // Since token value has increased, he can buy less shares for the same amount
    assert!(shares.raw() < "20".parse().unwrap());
}

#[test]
fn leader_position_closed_with_loss() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "200").unwrap();
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

    // We are going to make a loss!
    market.exec_set_price("0.5".try_into().unwrap()).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    market
        .query_closed_position(&market.copy_trading_addr, position_ids[0])
        .unwrap();

    let all_position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();

    assert_eq!(all_position_ids.len(), 0);

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let trader1 = market.clone_trader(1).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader1, "20")
        .unwrap();

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;

    assert!(tokens[0].collateral.diff("170".parse().unwrap()) < "0.1".parse().unwrap());

    // Reset work
    market.exec_copytrading_do_work(&trader1).unwrap();
    // Compute lp token value
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let token_event = response
        .events
        .iter()
        .find(|item| item.ty == "wasm-lp-token")
        .unwrap();
    let token_value = token_event
        .attributes
        .iter()
        .find(|item| item.key == "value")
        .unwrap();
    // Token value has increased
    assert!(Collateral::one() > token_value.value.parse().unwrap());
    // Do deposit
    market.exec_copytrading_do_work(&trader1).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert!(tokens[0].collateral.diff("190".parse().unwrap()) < "0.1".parse().unwrap());

    let tokens = market.query_copy_trading_balance(&trader1).unwrap();
    let shares = tokens.balance[0].shares;
    // Since token value has decreased, he can buy more shares for the same amount
    assert!(shares.raw() > "20".parse().unwrap());
}

#[test]
fn leader_withdrawal() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "200").unwrap();

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

    // We are going to make a profit!
    market.exec_set_price("1.5".try_into().unwrap()).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let trader1 = market.clone_trader(1).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader1, "20")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    match work {
        WorkResp::NoWork => panic!("Impossible: No work"),
        WorkResp::HasWork { work_description } => assert!(work_description.is_rebalance()),
    }

    // Rebalance the market
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-rebalanced")
        .add_attribute("made-profit", true.to_string())
        .add_attribute("batched", false.to_string());
    response.assert_event(&event);

    let leader_status = market.query_copy_trading_leader_tokens().unwrap();
    let commission = leader_status.tokens[0].unclaimed_commission;

    // Try to steal 0.1 collateral additonally
    let steal_commission = commission.checked_add("0.1".parse().unwrap()).unwrap();
    market
        .exec_copytrading_leader_withdraw(steal_commission)
        .unwrap_err();

    let valid_commission = commission.checked_div_dec("2".parse().unwrap()).unwrap();
    market
        .exec_copytrading_leader_withdraw(valid_commission)
        .unwrap();

    let leader_status = market.query_copy_trading_leader_tokens().unwrap();
    let new_commission = leader_status.tokens[0].unclaimed_commission;
    assert_eq!(
        new_commission.checked_add(valid_commission).unwrap(),
        commission
    );
}

fn make_profit(market: &PerpsMarket, trader: &Addr) -> anyhow::Result<ClosedPosition> {
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Leader opens a position
    market
        .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
        .unwrap();

    // Process queue item: Open the position
    market.exec_copytrading_do_work(trader).unwrap();
    market.exec_crank_till_finished(trader).unwrap();

    // Process queue item: Handle deferred exec id
    market.exec_copytrading_do_work(trader).unwrap();

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();

    // We are going to make a profit!
    market.exec_set_price("1.5".try_into().unwrap()).unwrap();
    market.exec_crank_till_finished(trader).unwrap();

    let closed_position = market
        .query_closed_position(&market.copy_trading_addr, position_ids[0])
        .unwrap();

    // Restore the price back
    market.exec_set_price("1".try_into().unwrap()).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    Ok(closed_position)
}

fn deposit_and_rebalance(market: &PerpsMarket, trader: &Addr) {
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    market
        .exec_copytrading_mint_and_deposit(trader, "20")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    match work {
        WorkResp::NoWork => panic!("Impossible: No work"),
        WorkResp::HasWork { work_description } => assert!(work_description.is_rebalance()),
    }
    // Rebalance work
    market.exec_copytrading_do_work(trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    if work.is_reset_status() {
        market.exec_copytrading_do_work(trader).unwrap();
    }
    // Compute lp token value
    market.exec_copytrading_do_work(trader).unwrap();
    // Do deposit
    market.exec_copytrading_do_work(trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
}

#[test]
fn no_double_comission() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "200").unwrap();

    let closed_position = make_profit(&market, &trader).unwrap();
    deposit_and_rebalance(&market, &trader);
    let trader1 = market.clone_trader(1).unwrap();

    let leader_comission = closed_position
        .pnl_collateral
        .try_into_non_negative_value()
        .unwrap()
        .checked_mul_dec("0.1".parse().unwrap())
        .unwrap();

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(leader_comission, tokens[0].unclaimed_commission);

    make_profit(&market, &trader).unwrap();
    deposit_and_rebalance(&market, &trader1);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let total_commission = status.tokens[0].unclaimed_commission;
    assert!(leader_comission
        .checked_mul_dec("2".parse().unwrap())
        .unwrap()
        .approx_eq(total_commission));
    assert_eq!(status.tokens[0].claimed_commission, Collateral::zero());
}
