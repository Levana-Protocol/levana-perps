use std::str::FromStr;

use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::copy_trading::QueueItem,
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
            funds: NonZero::new(Collateral::from_str("100").unwrap()).unwrap()
        }
    );
    assert!(response.processed_till.is_none())
}
