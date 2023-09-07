use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Address, Contract, Cosmos, HasAddress,
};
use msg::{
    contracts::pyth_bridge::{
        entry::{FeedType, QueryMsg as BridgeQueryMsg},
        MarketPrice,
    },
    prelude::*,
};
use shared::{namespace::PYTH_PREV_MARKET_PRICE, storage::map_key};

use perps_exes::config::PythMarketPriceFeeds;

#[derive(Clone)]
pub(crate) struct Pyth {
    pub oracle: Contract,
    pub bridge: Contract,
    pub market_id: MarketId,
    pub market_price_feeds: PythMarketPriceFeeds,
    pub feed_type: FeedType,
}

impl std::fmt::Debug for Pyth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pyth")
            .field("market_id", &self.market_id)
            .field("oracle", &self.oracle.get_address())
            .field("bridge", &self.bridge.get_address())
            .field("price_feed", &self.market_price_feeds.feeds)
            .field(
                "price_feeds_usd",
                &format!("{:?}", self.market_price_feeds.feeds_usd),
            )
            .field("feed_type", &self.feed_type)
            .finish()
    }
}

impl Pyth {
    pub async fn new(cosmos: &Cosmos, bridge_addr: Address, market_id: MarketId) -> Result<Self> {
        let bridge = cosmos.make_contract(bridge_addr);
        let msg::contracts::pyth_bridge::entry::Config {
            pyth: oracle_addr,
            feeds,
            feeds_usd,
            feed_type,
            factory: _,
            update_age_tolerance_seconds: _,
            market,
        } = bridge.query(BridgeQueryMsg::Config {}).await?;
        anyhow::ensure!(market_id == market);
        let oracle =
            cosmos.make_contract(oracle_addr.as_str().parse().with_context(|| {
                format!("Invalid Pyth oracle contract from Config: {oracle_addr}")
            })?);

        Ok(Self {
            oracle,
            bridge,
            market_price_feeds: PythMarketPriceFeeds { feeds, feeds_usd },
            market_id,
            feed_type,
        })
    }

    pub async fn query_price(&self, age_tolerance_seconds: u32) -> Result<MarketPrice> {
        self.bridge
            .query(BridgeQueryMsg::MarketPrice {
                age_tolerance_seconds,
            })
            .await
    }

    pub async fn prev_market_price_timestamp(&self, market_id: &MarketId) -> Result<Timestamp> {
        let res = self
            .bridge
            .query_raw(map_key(PYTH_PREV_MARKET_PRICE, market_id))
            .await?;
        let price: MarketPrice = serde_json::from_slice(&res)?;
        Ok(Timestamp::from_seconds(
            price.latest_price_publish_time.try_into()?,
        ))
    }

    pub async fn get_bridge_update_msg(
        &self,
        sender: String,
        execs: Option<u32>,
    ) -> Result<MsgExecuteContract> {
        Ok(MsgExecuteContract {
            sender,
            contract: self.bridge.get_address_string(),
            msg: serde_json::to_vec(
                &msg::contracts::pyth_bridge::entry::ExecuteMsg::UpdatePrice {
                    execs,
                    rewards: None,
                    bail_on_error: false,
                },
            )?,
            funds: vec![],
        })
    }
}
