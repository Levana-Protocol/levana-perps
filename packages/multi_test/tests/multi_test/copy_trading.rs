use std::str::FromStr;

use cosmwasm_std::Event;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::copy_trading::{IncQueueItem, IncQueuePositionId, WorkResp},
    shared::number::{Collateral, NonZero},
};

#[test]
fn query_config() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market.query_copy_trading_config().unwrap();
}

#[test]
fn deposit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let token = market.get_copytrading_token().unwrap();

    let trader = market.clone_trader(0).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    let response = market
        .query_copy_trading_queue_status(trader.into(), None, None)
        .unwrap();
    assert_eq!(response.items.len(), 1);
    let item = &response.items[0].item;

    assert_eq!(
        item,
        &IncQueueItem::Deposit {
            funds: NonZero::new(Collateral::from_str("100").unwrap()).unwrap(),
            token
        }
    );
    assert!(response.processed_till.is_none())
}

#[test]
fn initial_no_work() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork)
}

#[test]
fn detect_process_queue_item_work() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let token = market.get_copytrading_token().unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    // Needs to compute lp token value for the initial deposit
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ComputeLpTokenValue {
                token
            }
        }
    );
    let resp = market.exec_copytrading_do_work(&trader).unwrap();
    assert!(resp.has_event(
        &Event::new("wasm-lp-token").add_attribute("value", Collateral::one().to_string())
    ));

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ProcessQueueItem {
                id: IncQueuePositionId::new(0)
            }
        }
    );
}

#[test]
fn do_actual_deposit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Compute LP token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // Process queue item: do the actual deposit
    let resp = market.exec_copytrading_do_work(&trader).unwrap();
    assert!(resp.has_event(
        &Event::new("wasm-deposit")
            .add_attribute("funds", "100".to_owned())
            .add_attribute("shares", "100".to_owned())
    ));

    // Should not find any work now
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let balance = market.query_copy_trading_balance(&trader).unwrap();
    assert_eq!(balance.balance.len(), 1);
    assert_eq!(balance.balance[0].shares, "100".parse().unwrap());
    let token = market.get_copytrading_token().unwrap();
    assert_eq!(balance.balance[0].token, token);

    let another_trader = market.clone_trader(1).unwrap();
    let balance = market.query_copy_trading_balance(&another_trader).unwrap();
    assert!(balance.balance.is_empty());
}

#[test]
fn does_not_compute_lp_token_work() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Compute initial LP token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // Process queue item: do the actual deposit
    market.exec_copytrading_do_work(&trader).unwrap();

    // Should not find any work now
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Now let's do another deposit, so that it has to compute lp token value
    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Should not compute LP token value since there has been no positions opened etc.
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ProcessQueueItem {
                id: IncQueuePositionId::new(1)
            }
        }
    );
    // Process queue item: Do actual deposit
    market.exec_copytrading_do_work(&trader).unwrap();
}

#[test]
fn do_withdraw() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Compute LP token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // Process queue item: do the actual deposit
    market.exec_copytrading_do_work(&trader).unwrap();

    let initial_balance = market.query_copy_trading_balance(&trader).unwrap();
    assert_eq!(initial_balance.balance[0].shares, "100".parse().unwrap());

    market
        .exec_copytrading_withdrawal(&trader, "101")
        .unwrap_err();
    market.exec_copytrading_withdrawal(&trader, "50").unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ProcessQueueItem {
                id: IncQueuePositionId::new(1)
            }
        }
    );
    // Process queue item: do the actual withdrawal
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let new_balance = market.query_copy_trading_balance(&trader).unwrap();
    assert_eq!(new_balance.balance[0].shares, "50".parse().unwrap());

    market
        .exec_copytrading_withdrawal(&trader, "51")
        .unwrap_err();
    market.exec_copytrading_withdrawal(&trader, "50").unwrap();
    // Process queue item: do the actual withdrawal
    market.exec_copytrading_do_work(&trader).unwrap();
    market
        .exec_copytrading_withdrawal(&trader, "1")
        .unwrap_err();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
}
