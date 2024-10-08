use cosmos::{Contract, Cosmos, HasAddress};
use perps_exes::prelude::MarketContract;
use perpswap::contracts::factory::entry::{MarketInfoResponse, MarketsResp};
use perpswap::contracts::market::config::Config;
use perpswap::prelude::*;
use std::collections::HashSet;
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
    pub(crate) config: Config,
}

impl Debug for Market {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Market")
            .field("contract", &self.market.get_address())
            .field("name", &self.market_id)
            .finish()
    }
}

pub(crate) async fn get_markets(
    cosmos: &Cosmos,
    factory: &Contract,
    ignored_markets: &HashSet<MarketId>,
) -> Result<Vec<Market>> {
    let mut res = vec![];
    let mut start_after = None;

    loop {
        let MarketsResp { markets } = factory
            .query(perpswap::contracts::factory::entry::QueryMsg::Markets {
                start_after: start_after.take(),
                limit: None,
            })
            .await?;
        match markets.last() {
            None => break,
            Some(market) => start_after = Some(market.clone()),
        }

        for market_id in markets {
            if ignored_markets.contains(&market_id) {
                continue;
            }
            let MarketInfoResponse {
                market_addr,
                position_token,
                liquidity_token_lp,
                liquidity_token_xlp,
            } = factory
                .query(perpswap::contracts::factory::entry::QueryMsg::MarketInfo {
                    market_id: market_id.clone(),
                })
                .await?;
            let market =
                MarketContract::new(cosmos.make_contract(market_addr.into_string().parse()?));
            if !market.is_wound_down().await? {
                res.push(Market {
                    config: market.status().await?.config,
                    market,
                    position_token: cosmos.make_contract(position_token.into_string().parse()?),
                    liquidity_token_lp: cosmos
                        .make_contract(liquidity_token_lp.into_string().parse()?),
                    liquidity_token_xlp: cosmos
                        .make_contract(liquidity_token_xlp.into_string().parse()?),
                    market_id,
                });
            }
        }
    }
    Ok(res)
}
