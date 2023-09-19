use cosmos::{Contract, Cosmos, HasAddress};
use msg::contracts::factory::entry::{MarketInfoResponse, MarketsResp};
use msg::prelude::*;
use perps_exes::prelude::MarketContract;
use std::fmt::Debug;

#[derive(Clone)]
pub(crate) struct Market {
    pub(crate) market: MarketContract,
    #[allow(dead_code)]
    pub(crate) position_token: Contract,
    #[allow(dead_code)]
    pub(crate) liquidity_token_lp: Contract,
    #[allow(dead_code)]
    pub(crate) liquidity_token_xlp: Contract,
    pub(crate) market_id: MarketId,
}

impl Debug for Market {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Market")
            .field("contract", &self.market.get_address())
            .field("name", &self.market_id)
            .finish()
    }
}

pub(crate) async fn get_markets(cosmos: &Cosmos, factory: &Contract) -> Result<Vec<Market>> {
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
            let MarketInfoResponse {
                market_addr,
                position_token,
                liquidity_token_lp,
                liquidity_token_xlp,
            } = factory
                .query(msg::contracts::factory::entry::QueryMsg::MarketInfo {
                    market_id: market_id.clone(),
                })
                .await?;
            res.push(Market {
                market: MarketContract::new(
                    cosmos.make_contract(market_addr.into_string().parse()?),
                ),
                position_token: cosmos.make_contract(position_token.into_string().parse()?),
                liquidity_token_lp: cosmos.make_contract(liquidity_token_lp.into_string().parse()?),
                liquidity_token_xlp: cosmos
                    .make_contract(liquidity_token_xlp.into_string().parse()?),
                market_id,
            });
        }
    }
    Ok(res)
}
