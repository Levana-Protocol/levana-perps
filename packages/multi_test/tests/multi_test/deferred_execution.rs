//! All tests ultimately end up hitting deferred exeuction. The purpose of this module is to provide tests that can be used during the migration to deferred execution in the rest of the test suite.

use crate::prelude::*;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::contracts::market::deferred_execution::{DeferredExecFailure, DeferredExecStatus};

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
    assert_eq!(
        exec.status,
        DeferredExecStatus::Failure {
            reason: DeferredExecFailure::NotYetImplemented
        }
    );
    assert_eq!(&exec.owner, &trader);

    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);

    // FIXME update this test when we start implementing proper execution
}
