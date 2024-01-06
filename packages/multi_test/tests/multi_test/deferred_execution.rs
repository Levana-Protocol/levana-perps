//! All tests ultimately end up hitting deferred exeuction. The purpose of this module is to provide tests that can be used during the migration to deferred execution in the rest of the test suite.

use std::ops::{Mul, Sub};

use cosmwasm_std::{to_binary, WasmMsg};
use levana_perpswap_multi_test::{
    config::{SpotPriceKind, DEFAULT_MARKET},
    market_wrapper::{DeferResponse, PerpsMarket},
    position_helpers::assert_position_liquidated,
    response::CosmosResponseExt,
    time::TimeJump,
    PerpsApp,
};
use msg::{
    contracts::market::{
        deferred_execution::DeferredExecStatus,
        entry::{ExecuteMsg as MarketExecuteMsg, PositionsQueryFeeApproach},
        position::{events::PositionUpdateEvent, PositionId},
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

    market.exec_crank_till_finished(&cranker).unwrap();
    assert_eq!(market.query_status().unwrap().next_crank, None);

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
    let status = market.query_status().unwrap();
    assert_eq!(status.deferred_execution_items, 1);
    assert_eq!(status.last_processed_deferred_exec_id, None);
    assert_eq!(status.next_crank, None);

    // Now update the price...
    market.exec_refresh_price().unwrap();

    let status = market.query_status().unwrap();
    assert_eq!(status.deferred_execution_items, 1);
    assert_eq!(status.last_processed_deferred_exec_id, None);
    assert_ne!(status.next_crank, None);

    // and crank!
    market.exec_crank_till_finished(&cranker).unwrap();
    assert_eq!(market.query_status().unwrap().next_crank, None);

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

    let status = market.query_status().unwrap();
    assert_eq!(status.deferred_execution_items, 0);
    assert_eq!(
        status.last_processed_deferred_exec_id,
        Some("1".parse().unwrap())
    );
    assert_eq!(status.next_crank, None);
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
    let market = PerpsMarket::new_with_type(
        PerpsApp::new_cell().unwrap(),
        DEFAULT_MARKET.collateral_type,
        true,
        SpotPriceKind::Oracle,
    )
    .unwrap();
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    let open_position_oracle = || -> (PositionId, DeferResponse) {
        market
            .exec_open_position_refresh_price(
                &trader,
                "100",
                "9",
                DirectionToBase::Long,
                "1.0",
                None,
                None,
                None,
            )
            .unwrap()
    };

    let (pos_1, _) = open_position_oracle();
    let (pos_2, _) = open_position_oracle();
    let (pos_liquifund_only, _) = open_position_oracle();
    let init_liquifunded_at = market
        .query_position(pos_liquifund_only)
        .unwrap()
        .liquifunded_at;

    market.exec_crank_till_finished(&cranker).unwrap();

    let update_queue_1 = market
        .exec_update_position_leverage_queue_only(&trader, pos_1, "10".parse().unwrap(), None)
        .unwrap();

    // the queue above did not move forward - gotta set the price at the _next_ block
    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_refresh_price().unwrap();
    market.set_time(TimeJump::Liquifundings(1)).unwrap();

    let first_liquifunding_time = market.now();

    let update_queue_2 = market
        .exec_update_position_leverage_queue_only(&trader, pos_2, "10".parse().unwrap(), None)
        .unwrap();

    // the queue above did not move forward - gotta set the price at the _next_ block
    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_refresh_price().unwrap();
    market.set_time(TimeJump::Liquifundings(1)).unwrap();

    assert_eq!(market.query_status().unwrap().deferred_execution_items, 2);
    assert!(market
        .query_deferred_exec(update_queue_1.value.id)
        .unwrap()
        .status
        .is_pending());
    assert!(market
        .query_deferred_exec(update_queue_2.value.id)
        .unwrap()
        .status
        .is_pending());

    // This crank should process the first update queue - step 7/8 in the jira issue
    market.exec_refresh_price().unwrap(); // we have to refresh the price first though, otherwise it's too old and the cranking will fail
    market.exec_crank_n(&cranker, 100).unwrap();

    assert_eq!(market.query_status().unwrap().deferred_execution_items, 1);
    assert!(!market
        .query_deferred_exec(update_queue_1.value.id)
        .unwrap()
        .status
        .is_pending());
    assert!(market
        .query_deferred_exec(update_queue_2.value.id)
        .unwrap()
        .status
        .is_pending());

    // confirm that no liquifundings have happened yet - step 8 in the jira issue
    let liquifunded_at = market
        .query_position(pos_liquifund_only)
        .unwrap()
        .liquifunded_at;
    assert_eq!(liquifunded_at, init_liquifunded_at);

    // This crank should process the first liquifunding and the second update - step 9/10 in the jira issue
    market.exec_crank_n(&cranker, 100).unwrap();
    assert_eq!(market.query_status().unwrap().deferred_execution_items, 0);
    assert!(!market
        .query_deferred_exec(update_queue_1.value.id)
        .unwrap()
        .status
        .is_pending());
    assert!(!market
        .query_deferred_exec(update_queue_2.value.id)
        .unwrap()
        .status
        .is_pending());

    // Confirm that we've only processed the first liquifunding
    let liquifunded_at = market
        .query_position(pos_liquifund_only)
        .unwrap()
        .liquifunded_at;
    assert!(liquifunded_at > init_liquifunded_at);
    assert!(liquifunded_at <= first_liquifunding_time);

    // final crank - i.e. step 11 in the jira issue
    market.exec_crank_n(&cranker, 100).unwrap();
    assert!(market.query_status().unwrap().next_crank.is_none());

    let last_liquifunded_at = market
        .query_position(pos_liquifund_only)
        .unwrap()
        .liquifunded_at;
    assert!(last_liquifunded_at > liquifunded_at);
    assert!(last_liquifunded_at > first_liquifunding_time);
}

#[test]
fn defer_before_crank_2855() {
    let market = PerpsMarket::new_with_type(
        PerpsApp::new_cell().unwrap(),
        DEFAULT_MARKET.collateral_type,
        true,
        SpotPriceKind::Oracle,
    )
    .unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    // create a position so there will be some work to do, i.e. liquifunding
    let (pos_id, _) = market
        .exec_open_position_refresh_price(
            &trader,
            "100",
            "9",
            DirectionToBase::Long,
            "8",
            None,
            None,
            None,
        )
        .unwrap();

    market.exec_crank_till_finished(&cranker).unwrap();

    let initial_active_collateral = market
        .query_position(pos_id)
        .unwrap()
        .active_collateral
        .into_number();

    // queue an update and close with some time in between
    market
        .exec_update_position_leverage_queue_only(&trader, pos_id, "12".parse().unwrap(), None)
        .unwrap();
    market.set_time(TimeJump::Blocks(1)).unwrap();
    let close_resp = market
        .exec_close_position_queue_only(&trader, pos_id, None)
        .unwrap();

    // no pnl accumulated, update and close are both deferred
    let pos = market.query_position(pos_id).unwrap();
    assert_eq!(
        pos.active_collateral.into_number(),
        initial_active_collateral
    );
    assert!(pos.leverage < "10".parse().unwrap());
    assert_eq!(market.query_status().unwrap().deferred_execution_items, 2);
    market.query_closed_position(&trader, pos_id).unwrap_err();
    close_resp
        .response
        .event_first("position-update")
        .unwrap_err();

    // crank without a price point here works but doesn't do anything - i.e. same state as before
    market.exec_crank_n(&cranker, 100).unwrap();
    let pos = market.query_position(pos_id).unwrap();
    assert_eq!(
        pos.active_collateral.into_number(),
        initial_active_collateral
    );
    assert!(pos.leverage < "10".parse().unwrap());
    assert_eq!(market.query_status().unwrap().deferred_execution_items, 2);
    market.query_closed_position(&trader, pos_id).unwrap_err();
    close_resp
        .response
        .event_first("position-update")
        .unwrap_err();

    // double the price
    let price_timestamp = market.now();
    market
        .exec_set_price(
            PriceBaseInQuote::try_from_number(
                market
                    .query_current_price()
                    .unwrap()
                    .price_base
                    .into_number()
                    .checked_mul("1.5".parse().unwrap())
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    // One block's worth of cranking is enough to churn through the update and close - because there's only one price point
    let res = market.exec_crank_n(&cranker, 100).unwrap();

    // now position is updated
    let update_event: PositionUpdateEvent = res
        .event_first("position-update")
        .unwrap()
        .try_into()
        .unwrap();

    // active collateral increased by a lot in the update (much more than just the updated delta, i.e. includes PnL from price)
    let update_active_collateral = update_event
        .position_attributes
        .collaterals
        .active_collateral
        .into_number();

    assert!(update_active_collateral > initial_active_collateral);
    assert!(
        update_active_collateral.sub(initial_active_collateral)
            > (update_event
                .active_collateral_delta
                .into_number()
                .mul(Number::from_str("10").unwrap()))
            .abs()
    );

    // position was also closed
    let closed_pos = market.query_closed_position(&trader, pos_id).unwrap();

    // and it was closed with more collateral than what it had at the update
    assert!(closed_pos.active_collateral.into_number() > update_active_collateral);

    // last crank has the fee updates, and it's historical
    let res = market.exec_crank_till_finished(&cranker).unwrap();
    let funding_rate_timestamp: Timestamp = res
        .event_first_value("funding-rate-change", "time")
        .unwrap()
        .parse()
        .unwrap();

    assert_eq!(price_timestamp, funding_rate_timestamp);
}

#[test]
fn defer_liquidation_2856() {
    let market = PerpsMarket::new_with_type(
        PerpsApp::new_cell().unwrap(),
        DEFAULT_MARKET.collateral_type,
        true,
        SpotPriceKind::Oracle,
    )
    .unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();

    market.exec_set_price("10".try_into().unwrap()).unwrap();

    let (pos_id, _) = market
        .exec_open_position_refresh_price(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "0.5",
            None,
            None,
            None,
        )
        .unwrap();

    market.exec_crank_till_finished(&cranker).unwrap();

    // check that we go from "open" to "pending close" but NOT "closed" when we push the new price
    market.query_position(pos_id).unwrap();
    market.exec_set_price("100".parse().unwrap()).unwrap();
    market.query_position(pos_id).unwrap_err();
    market
        .query_position_pending_close(pos_id, PositionsQueryFeeApproach::Accumulated)
        .unwrap();
    market.query_closed_position(&trader, pos_id).unwrap_err();

    // now queue an update
    let queue_resp = market
        .exec_update_position_leverage_queue_only(&trader, pos_id, "8".parse().unwrap(), None)
        .unwrap();
    assert!(queue_resp.value.status.is_pending());
    // make sure there's a valid price point for the update to be processed
    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_refresh_price().unwrap();

    // even though the update will not be allowed to go through, as of now it's still pending
    assert!(market
        .query_deferred_exec(queue_resp.value.id)
        .unwrap()
        .status
        .is_pending());

    // crank it all out
    market.exec_crank_till_finished(&trader).unwrap();

    // the update failed
    match market
        .query_deferred_exec(queue_resp.value.id)
        .unwrap()
        .status
    {
        DeferredExecStatus::Failure { .. } => {}
        _ => panic!("Unexpected status, should have failed"),
    }

    // and position is fully closed due to liquidation
    let pos = market.query_closed_position(&trader, pos_id).unwrap();
    assert_position_liquidated(&pos).unwrap();
    market.query_position(pos_id).unwrap_err();
    market
        .query_position_pending_close(pos_id, PositionsQueryFeeApproach::Accumulated)
        .unwrap_err();
}
