//! All tests ultimately end up hitting deferred exeuction. The purpose of this module is to provide tests that can be used during the migration to deferred execution in the rest of the test suite.

use crate::prelude::*;
use cosmwasm_std::{to_binary, WasmMsg};
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::contracts::market::deferred_execution::DeferredExecStatus;

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

    // Since we haven't implemented anything yet, for now assert that there are no positions and an error.
    let execs = market.query_deferred_execs(&trader).unwrap();
    assert_eq!(execs.len(), 1);
    let exec = execs.into_iter().next().unwrap();
    match exec.status {
        DeferredExecStatus::Pending => panic!("Unexpected pending"),
        DeferredExecStatus::Success { .. } => panic!("Unexpected success"),
        DeferredExecStatus::Failure { .. } => (),
    }
    assert_eq!(&exec.owner, &trader);

    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);

    // FIXME update this test when we start implementing proper execution
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
                msg: to_binary(&MarketExecuteMsg::PerformDeferredExec { id: exec.id }).unwrap(),
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
