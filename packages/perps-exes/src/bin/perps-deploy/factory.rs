use anyhow::Result;
use cosmos::proto::cosmos::base::abci::v1beta1::TxResponse;
use cosmos::{Address, Contract, HasCosmos, Wallet};
use msg::contracts::factory::entry::{FactoryOwnerResp, MarketInfoResponse, MarketsResp, QueryMsg};
use msg::prelude::*;
use msg::shutdown::{ShutdownEffect, ShutdownImpact};

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

    pub(crate) async fn query_migration_admin(&self) -> Result<Address> {
        let FactoryOwnerResp {
            admin_migration, ..
        } = self.0.query(QueryMsg::FactoryOwner {}).await?;
        admin_migration.into_string().parse().with_context(|| {
            format!(
                "Invalid factory migration admin found for factory {}",
                self.0
            )
        })
    }

    pub(crate) async fn disable_trades(
        &self,
        wallet: &Wallet,
        market: MarketId,
    ) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                FactoryExecuteMsg::Shutdown {
                    markets: vec![market],
                    impacts: vec![ShutdownImpact::NewTrades],
                    effect: ShutdownEffect::Disable,
                },
            )
            .await
    }

    pub(crate) async fn enable_all(&self, wallet: &Wallet) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                FactoryExecuteMsg::Shutdown {
                    markets: vec![],
                    impacts: vec![],
                    effect: ShutdownEffect::Enable,
                },
            )
            .await
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
