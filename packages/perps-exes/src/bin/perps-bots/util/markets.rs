use std::fmt::Debug;

use cosmos::{Address, Contract, Cosmos, HasAddress};
use msg::contracts::factory::entry::{MarketInfoResponse, MarketsResp};
use msg::prelude::*;

#[derive(Clone)]
pub(crate) struct Market {
    pub(crate) market: Contract,
    pub(crate) position_token: Contract,
    #[allow(dead_code)]
    pub(crate) liquidity_token_lp: Contract,
    #[allow(dead_code)]
    pub(crate) liquidity_token_xlp: Contract,
    pub(crate) price_api_symbol: String,
    pub(crate) collateral_price_api_symbol: Option<String>,
    pub(crate) market_id: MarketId,
}

impl Debug for Market {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Market")
            .field("contract", self.market.get_address())
            .field("price_api_symbol", &self.price_api_symbol)
            .field("name", &self.market_id)
            .finish()
    }
}

pub(crate) async fn get_markets(cosmos: &Cosmos, factory: Address) -> Result<Vec<Market>> {
    let factory = cosmos.make_contract(factory);
    let mut res = vec![];
    let mut start_after = None;

    loop {
        let MarketsResp { markets } = factory
            .query(msg::contracts::factory::entry::QueryMsg::Markets {
                start_after: start_after.take(),
                limit: None,
            })
            .await?;
        match markets.last() {
            None => break,
            Some(market) => start_after = Some(market.clone()),
        }
        for market_id in markets {
            let price_api_symbol = if market_id.get_quote() == "USDC" {
                format!("{}_USD", market_id.get_base())
            } else {
                market_id.to_string()
            };
            let collateral_price_api_symbol = if market_id.is_notional_usd() {
                None
            } else {
                Some(format!("{}_USD", market_id.get_collateral()))
            };

            let MarketInfoResponse {
                market_addr,
                position_token,
                liquidity_token_lp,
                liquidity_token_xlp,
                price_admin: _,
            } = factory
                .query(msg::contracts::factory::entry::QueryMsg::MarketInfo {
                    market_id: market_id.clone(),
                })
                .await?;
            res.push(Market {
                market: cosmos.make_contract(market_addr.into_string().parse()?),
                position_token: cosmos.make_contract(position_token.into_string().parse()?),
                liquidity_token_lp: cosmos.make_contract(liquidity_token_lp.into_string().parse()?),
                liquidity_token_xlp: cosmos
                    .make_contract(liquidity_token_xlp.into_string().parse()?),
                price_api_symbol,
                market_id,
                collateral_price_api_symbol,
            });
        }
    }
    Ok(res)
}
