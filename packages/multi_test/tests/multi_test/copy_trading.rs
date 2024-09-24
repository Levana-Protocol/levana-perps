use std::str::FromStr;

use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::copy_trading::{QueueItem, QueuePositionId, WorkResp},
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
        &QueueItem::Deposit {
            funds: NonZero::new(Collateral::from_str("100").unwrap()).unwrap(),
            token: msg::contracts::copy_trading::Token::Native(TEST_CONFIG.native_denom.clone())
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

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ProcessQueueItem {
                id: QueuePositionId::new(0)
            }
        }
    )
}

#[test]
fn do_actual_deposit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Process queue item
    market.exec_copytrading_do_work(&trader).unwrap();
}
