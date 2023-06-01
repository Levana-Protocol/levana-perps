use anyhow::Result;
use cosmos::{Address, Contract, HasCosmos};
use msg::contracts::factory::entry::{FactoryOwnerResp, MarketInfoResponse, MarketsResp, QueryMsg};
use msg::prelude::*;

pub(crate) struct Factory(Contract);

impl Factory {
    pub(crate) fn from_contract(contract: Contract) -> Self {
        Factory(contract)
    }

    pub(crate) async fn get_market(&self, market_id: impl Into<MarketId>) -> Result<MarketInfo> {
        let market_id = market_id.into();
        let MarketInfoResponse {
            market_addr,
            position_token,
            liquidity_token_lp,
            liquidity_token_xlp,
            price_admin,
        } = self
            .0
            .query(QueryMsg::MarketInfo {
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

        Ok(MarketInfo {
            market_id,
            market,
            position_token,
            liquidity_token_lp,
            liquidity_token_xlp,
            price_admin: price_admin.into_string().parse()?,
        })
    }

    pub(crate) async fn get_markets(&self) -> Result<Vec<MarketInfo>> {
        let mut start_after = None;
        let mut res = vec![];

        loop {
            let MarketsResp { markets } = self
                .0
                .query(QueryMsg::Markets {
                    start_after: start_after.take(),
                    limit: None,
                })
                .await?;

            match markets.last() {
                Some(market_id) => start_after = Some(market_id.clone()),
                None => break,
            }

            for market_id in markets {
                res.push(self.get_market(market_id).await?);
            }
        }

        Ok(res)
    }

    pub(crate) async fn query_owner(&self) -> Result<Address> {
        let FactoryOwnerResp { owner, .. } = self.0.query(QueryMsg::FactoryOwner {}).await?;
        owner
            .into_string()
            .parse()
            .with_context(|| format!("Invalid factory owner found for factory {}", self.0))
    }
}

pub(crate) struct MarketInfo {
    pub(crate) market_id: MarketId,
    pub(crate) market: Contract,
    pub(crate) position_token: Contract,
    pub(crate) liquidity_token_lp: Contract,
    pub(crate) liquidity_token_xlp: Contract,
    pub(crate) price_admin: Address,
}
