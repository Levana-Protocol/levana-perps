use cosmos::{Address, Contract, Cosmos, HasAddress, Wallet};
use msg::contracts::factory::entry::{MarketInfoResponse, MarketsResp};
use msg::prelude::*;
use std::fmt::Debug;

use crate::config::BotConfig;

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
pub(crate) enum PriceApi {
    Pyth(Pyth),
    Manual {
        symbol: String,
        symbol_usd: Option<String>,
    },
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
    pub(crate) async fn get_price_api(
        &self,
        wallet: &Wallet,
        cosmos: &Cosmos,
        config: &BotConfig,
    ) -> Result<PriceApi> {
        let Self {
            price_admin,
            market_id,
            ..
        } = self;

        if *price_admin == wallet.get_address_string() {
            let symbol = if market_id.get_quote() == "USDC" {
                format!("{}_USD", market_id.get_base())
            } else {
                market_id.to_string()
            };

            let symbol_usd = if market_id.is_notional_usd() {
                None
            } else {
                Some(format!("{}_USD", market_id.get_collateral()))
            };

            Ok(PriceApi::Manual { symbol, symbol_usd })
        } else {
            let bridge_addr = Address::from_str(price_admin)?;
            let pyth = Pyth::new(cosmos, config, bridge_addr, market_id.clone()).await?;
            Ok(PriceApi::Pyth(pyth))
        }
    }
}
