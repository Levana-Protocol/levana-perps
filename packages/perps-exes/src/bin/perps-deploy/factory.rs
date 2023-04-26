use anyhow::Result;
use cosmos::{Contract, HasCosmos};
use msg::contracts::factory::entry::{MarketInfoResponse, MarketsResp};
use msg::prelude::*;

pub(crate) struct Factory(Contract);

impl Factory {
    pub(crate) fn from_contract(contract: Contract) -> Self {
        Factory(contract)
    }

    pub(crate) async fn get_markets(&self) -> Result<Vec<MarketInfo>> {
        let mut start_after = None;
        let mut res = vec![];

        loop {
            let MarketsResp { markets } = self
                .0
                .query(msg::contracts::factory::entry::QueryMsg::Markets {
                    start_after: start_after.take(),
                    limit: None,
                })
                .await?;

            match markets.last() {
                Some(market_id) => start_after = Some(market_id.clone()),
                None => break,
            }

            for market_id in markets {
                let MarketInfoResponse {
                    market_addr,
                    position_token,
                    liquidity_token_lp,
                    liquidity_token_xlp,
                    price_admin: _,
                } = self
                    .0
                    .query(msg::contracts::factory::entry::QueryMsg::MarketInfo {
                        market_id: market_id.clone(),
                    })
                    .await?;

                let market = self
                    .0
                    .get_cosmos()
                    .make_contract(market_addr.as_str().parse()?);
                let position_token = self
                    .0
                    .get_cosmos()
                    .make_contract(position_token.as_str().parse()?);
                let liquidity_token_lp = self
                    .0
                    .get_cosmos()
                    .make_contract(liquidity_token_lp.as_str().parse()?);
                let liquidity_token_xlp = self
                    .0
                    .get_cosmos()
                    .make_contract(liquidity_token_xlp.as_str().parse()?);

                res.push(MarketInfo {
                    market_id,
                    market,
                    position_token,
                    liquidity_token_lp,
                    liquidity_token_xlp,
                })
            }
        }

        Ok(res)
    }
}

pub(crate) struct MarketInfo {
    pub(crate) market_id: MarketId,
    pub(crate) market: Contract,
    pub(crate) position_token: Contract,
    pub(crate) liquidity_token_lp: Contract,
    pub(crate) liquidity_token_xlp: Contract,
}
