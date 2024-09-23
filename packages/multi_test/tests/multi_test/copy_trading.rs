use std::str::FromStr;

use cosmwasm_std::{Addr, Decimal256};
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::countertrade::{ConfigUpdate, HasWorkResp, MarketBalance, WorkDescription},
    prelude::{DirectionToBase, Number, TakeProfitTrader, UnsignedDecimal, Usd},
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
}
