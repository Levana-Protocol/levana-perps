//! All tests ultimately end up hitting deferred exeuction. The purpose of this module is to provide tests that can be used during the migration to deferred execution in the rest of the test suite.

use cosmwasm_std::{to_binary, WasmMsg};
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp, time::TimeJump};
use msg::{
    contracts::market::{
        deferred_execution::DeferredExecStatus, entry::ExecuteMsg as MarketExecuteMsg,
    },
    prelude::*,
    shared::storage::DirectionToBase,
};

#[test]
fn basic_operations() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_lp(0).unwrap();

    assert_eq!(market.query_deferred_execs(&trader).unwrap(), vec![]);

    let msg = market
        .token
        .into_market_execute_msg(
            &market.addr,
            "100".parse().unwrap(),
            MarketExecuteMsg::OpenPosition {
                slippage_assert: None,
                leverage: "10".parse().unwrap(),
                direction: DirectionToBase::Long,
                max_gains: MaxGainsInQuote::Finite("1.2".parse().unwrap()),
                stop_loss_override: None,
                take_profit_override: None,
            },
        )
        .unwrap();
    market.exec_wasm_msg(&trader, msg).unwrap();

    // First, make sure this item is sitting on the queue
    let execs = market.query_deferred_execs(&trader).unwrap();
    assert_eq!(execs.len(), 1);
    let exec = execs.into_iter().next().unwrap();
    assert_eq!(exec.status, DeferredExecStatus::Pending);
    assert_eq!(&exec.owner, &trader);

    // Now crank
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    let execs = market.query_deferred_execs(&trader).unwrap();
    assert_eq!(execs.len(), 1);
    let exec = execs.into_iter().next().unwrap();
    match exec.status {
        DeferredExecStatus::Pending => panic!("Unexpected pending"),
        DeferredExecStatus::Success { .. } => (),
        DeferredExecStatus::Failure { .. } => panic!("Unexpected failure"),
    }
    assert_eq!(&exec.owner, &trader);

    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 1);
    let position = positions.into_iter().next().unwrap();
    assert_eq!(position.owner, trader);
}

#[test]
fn cannot_perform_deferred_exec() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_lp(0).unwrap();

    assert_eq!(market.query_deferred_execs(&trader).unwrap(), vec![]);

    let msg = market
        .token
        .into_market_execute_msg(
            &market.addr,
            "100".parse().unwrap(),
            MarketExecuteMsg::OpenPosition {
                slippage_assert: None,
                leverage: "10".parse().unwrap(),
                direction: DirectionToBase::Long,
                max_gains: MaxGainsInQuote::Finite("1.2".parse().unwrap()),
                stop_loss_override: None,
                take_profit_override: None,
            },
        )
        .unwrap();
    market.exec_wasm_msg(&trader, msg).unwrap();

    // First, make sure this item is sitting on the queue
    let execs = market.query_deferred_execs(&trader).unwrap();
    assert_eq!(execs.len(), 1);
    let exec = execs.into_iter().next().unwrap();
    assert_eq!(exec.status, DeferredExecStatus::Pending);

    market
        .exec_wasm_msg(
            &cranker,
            WasmMsg::Execute {
                contract_addr: market.addr.clone().into_string(),
                msg: to_binary(&MarketExecuteMsg::PerformDeferredExec {
                    id: exec.id,
                    price_point_timestamp: Timestamp::default(),
                })
                .unwrap(),
                funds: vec![],
            },
        )
        .unwrap_err();
}

#[test]
fn replies_work_for_two_in_queue() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_lp(0).unwrap();

    assert_eq!(market.query_deferred_execs(&trader).unwrap(), vec![]);

    let msg = market
        .token
        .into_market_execute_msg(
            &market.addr,
            "100".parse().unwrap(),
            MarketExecuteMsg::OpenPosition {
                slippage_assert: None,
                leverage: "10".parse().unwrap(),
                direction: DirectionToBase::Long,
                max_gains: MaxGainsInQuote::Finite("1.2".parse().unwrap()),
                stop_loss_override: None,
                take_profit_override: None,
            },
        )
        .unwrap();
    market.exec_wasm_msg(&trader, msg.clone()).unwrap();
    market.exec_wasm_msg(&trader, msg).unwrap();

    let execs = market.query_deferred_execs(&trader).unwrap();
    assert_eq!(execs.len(), 2);

    // Now crank
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    let execs = market.query_deferred_execs(&trader).unwrap();
    for exec in execs {
        assert!(!exec.status.is_pending())
    }
}

#[test]
fn non_deferred_after_deferred_2853() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    let (pos_1, _) = market
        .exec_open_position(
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

    let (pos_2, _) = market
        .exec_open_position(
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

    market.exec_crank_till_finished(&cranker).unwrap();

    let update_queue_1 = market.exec_update_position_leverage_queue_only(&trader, pos_1, "10".parse().unwrap(), None).unwrap();
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    let update_queue_2 = market.exec_update_position_leverage_queue_only(&trader, pos_2, "10".parse().unwrap(), None).unwrap();
    market.set_time(TimeJump::Liquifundings(1)).unwrap();

    assert_eq!(market.query_status().unwrap().deferred_execution_items, 2);
    assert!(market.query_deferred_exec(update_queue_1.value.id).unwrap().status.is_pending());
    assert!(market.query_deferred_exec(update_queue_2.value.id).unwrap().status.is_pending());

    // At 4 cranks there's still 2 items left
    // At 5 cranks there's 1 item left
    // At 6 cranks there's 0 
    let res = market.exec_crank_n(&cranker, 6).unwrap();

    println!("{:#?}", res);
    println!("{:#?}", market.query_deferred_exec(update_queue_1.value.id).unwrap().status);
    println!("{:#?}", market.query_deferred_exec(update_queue_2.value.id).unwrap().status);

    assert_eq!(market.query_status().unwrap().deferred_execution_items, 1);
    // assert!(market.query_deferred_exec(update_queue_1.value.id).unwrap().status.is_pending());
    // assert!(market.query_deferred_exec(update_queue_2.value.id).unwrap().status.is_pending());


}