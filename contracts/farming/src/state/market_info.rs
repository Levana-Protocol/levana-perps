use cosmwasm_std::{Empty, QuerierWrapper};
use msg::{contracts::factory::entry::MarketInfoResponse, token::Token};

use crate::prelude::*;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct MarketInfo {
    pub(crate) addr: Addr,
    pub(crate) id: MarketId,
    pub(crate) collateral: Token,
    pub(crate) lp_addr: Addr,
    pub(crate) xlp_addr: Addr,
}

const MARKET_INFO: Item<MarketInfo> = Item::new("market-info");

impl MarketInfo {
    /// Load from this contract's storage.
    pub(crate) fn load(store: &dyn Storage) -> Result<Self> {
        MARKET_INFO.load(store).context("MARKET_INFO is empty")
    }

    /// Calculate from queries to the market contract and fill in this contract's storage.
    pub(crate) fn save(
        querier: QuerierWrapper<Empty>,
        store: &mut dyn Storage,
        factory: Addr,
        market_id: MarketId,
    ) -> Result<()> {
        let MarketInfoResponse {
            market_addr,
            position_token: _,
            liquidity_token_lp,
            liquidity_token_xlp,
        } = querier.query_wasm_smart(
            factory,
            &msg::contracts::factory::entry::QueryMsg::MarketInfo { market_id },
        )?;
        let status: msg::contracts::market::entry::StatusResp = querier.query_wasm_smart(
            market_addr.clone(),
            &msg::contracts::market::entry::QueryMsg::Status { price: None },
        )?;
        let info = MarketInfo {
            addr: market_addr,
            id: status.market_id,
            collateral: status.collateral,
            lp_addr: liquidity_token_lp,
            xlp_addr: liquidity_token_xlp,
        };
        MARKET_INFO.save(store, &info)?;
        Ok(())
    }
}
