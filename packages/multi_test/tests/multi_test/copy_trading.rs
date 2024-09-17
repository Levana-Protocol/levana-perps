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
