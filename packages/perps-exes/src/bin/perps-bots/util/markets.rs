use cosmos::{Address, Contract, Cosmos, HasAddress, Wallet};
use msg::contracts::factory::entry::{MarketInfoResponse, MarketsResp};
use msg::contracts::pyth_bridge::PythMarketPriceFeeds;
use msg::prelude::*;
use perps_exes::config::PythConfig;
use std::fmt::Debug;

use super::oracle::Pyth;

#[derive(Clone)]
pub(crate) struct Market {
    pub(crate) market: Contract,
    #[allow(dead_code)]
    pub(crate) position_token: Contract,
    #[allow(dead_code)]
    pub(crate) liquidity_token_lp: Contract,
    #[allow(dead_code)]
    pub(crate) liquidity_token_xlp: Contract,
    pub(crate) market_id: MarketId,
    pub(crate) price_admin: String,
}

impl Debug for Market {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Market")
            .field("contract", &self.market.get_address())
            .field("name", &self.market_id)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum PriceApi<'a> {
    Pyth(Pyth),
    Manual(&'a PythMarketPriceFeeds),
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
                price_admin,
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
                market_id,
                price_admin: price_admin.into_string(),
            });
        }
    }
    Ok(res)
}

impl Market {
    pub(crate) async fn get_price_api<'a>(
        &self,
        wallet: &Wallet,
        cosmos: &Cosmos,
        pyth_config: &'a PythConfig,
    ) -> Result<PriceApi<'a>> {
        let Self {
            price_admin,
            market_id,
            ..
        } = self;

        if *price_admin == wallet.get_address_string() {
            // Not using Pyth oracle, but still getting the prices from the Pyth endpoint
            let feeds = pyth_config
                .markets
                .get(market_id)
                .with_context(|| format!("No Pyth config found for market {market_id}"))?;

            Ok(PriceApi::Manual(feeds))
        } else {
            let bridge_addr = Address::from_str(price_admin)?;
            let pyth = Pyth::new(cosmos, bridge_addr, market_id.clone()).await?;
            Ok(PriceApi::Pyth(pyth))
        }
    }
}
