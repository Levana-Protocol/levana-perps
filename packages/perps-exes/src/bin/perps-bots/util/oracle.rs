use cosmos::{proto::cosmwasm::wasm::v1::MsgExecuteContract, Contract, Cosmos, HasAddress};
use msg::prelude::*;

use perps_exes::config::PythMarketPriceFeeds;

#[derive(Clone)]
pub(crate) struct Pyth {
    pub oracle: Contract,
    pub market_id: MarketId,
    pub market_price_feeds: PythMarketPriceFeeds,
}

impl std::fmt::Debug for Pyth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pyth")
            .field("market_id", &self.market_id)
            .field("oracle", &self.oracle.get_address())
            .field("price_feed", &self.market_price_feeds.feeds)
            .field(
                "price_feeds_usd",
                &format!("{:?}", self.market_price_feeds.feeds_usd),
            )
            .finish()
    }
}

impl Pyth {
    pub async fn new(_cosmos: &Cosmos, _market_id: MarketId) -> Result<Self> {
        todo!()
        // let msg::contracts::pyth_bridge::entry::Config {
        //     pyth: oracle_addr,
        //     feeds,
        //     feeds_usd,
        //     feed_type,
        //     factory: _,
        //     update_age_tolerance_seconds: _,
        //     market,
        // } = bridge.query(BridgeQueryMsg::Config {}).await?;
        // anyhow::ensure!(market_id == market);
        // let oracle =
        //     cosmos.make_contract(oracle_addr.as_str().parse().with_context(|| {
        //         format!("Invalid Pyth oracle contract from Config: {oracle_addr}")
        //     })?);

        // Ok(Self {
        //     oracle,
        //     bridge,
        //     market_price_feeds: PythMarketPriceFeeds { feeds, feeds_usd },
        //     market_id,
        //     feed_type,
        // })
    }

    pub async fn query_price(&self, _age_tolerance_seconds: u32) -> Result<PricePoint> {
        todo!()
        // self.bridge
        //     .query(BridgeQueryMsg::MarketPrice {
        //         age_tolerance_seconds,
        //     })
        //     .await
    }

    pub async fn prev_market_price_timestamp(&self, _market_id: &MarketId) -> Result<Timestamp> {
        todo!()
        // let res = self
        //     .bridge
        //     .query_raw(map_key(PYTH_PREV_MARKET_PRICE, market_id))
        //     .await?;
        // let price: MarketPrice = serde_json::from_slice(&res)?;
        // Ok(Timestamp::from_seconds(
        //     price.latest_price_publish_time.try_into()?,
        // ))
    }

    pub async fn get_bridge_update_msg(
        &self,
        _sender: String,
        _execs: Option<u32>,
    ) -> Result<MsgExecuteContract> {
        todo!()
        // Ok(MsgExecuteContract {
        //     sender,
        //     contract: self.bridge.get_address_string(),
        //     msg: serde_json::to_vec(
        //         &msg::contracts::pyth_bridge::entry::ExecuteMsg::UpdatePrice {
        //             execs,
        //             rewards: None,
        //             bail_on_error: false,
        //         },
        //     )?,
        //     funds: vec![],
        // })
    }
}
