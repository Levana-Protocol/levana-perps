use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Address, Contract, Cosmos, HasAddress,
};
use msg::{
    contracts::pyth_bridge::{
        entry::QueryMsg as BridgeQueryMsg, MarketPrice, PythMarketPriceFeeds,
    },
    prelude::*,
};

#[derive(Clone)]
pub(crate) struct Pyth {
    pub oracle: Contract,
    pub bridge: Contract,
    pub market_id: MarketId,
    pub market_price_feeds: PythMarketPriceFeeds,
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
            .finish()
    }
}

impl Pyth {
    pub async fn new(cosmos: &Cosmos, bridge_addr: Address, market_id: MarketId) -> Result<Self> {
        let bridge = cosmos.make_contract(bridge_addr);
        let oracle_addr = bridge.query(BridgeQueryMsg::PythAddress {}).await?;
        let oracle = cosmos.make_contract(oracle_addr);
        let market_price_feeds = bridge
            .query(BridgeQueryMsg::MarketPriceFeeds {
                market_id: market_id.clone(),
            })
            .await?;

        Ok(Self {
            oracle,
            bridge,
            market_price_feeds,
            market_id,
        })
    }

    pub async fn query_price(&self, age_tolerance_seconds: u32) -> Result<MarketPrice> {
        self.bridge
            .query(BridgeQueryMsg::MarketPrice {
                market_id: self.market_id.clone(),
                age_tolerance_seconds,
            })
            .await
    }

    pub async fn get_bridge_update_msg(
        &self,
        sender: String,
        market_id: MarketId,
    ) -> Result<MsgExecuteContract> {
        Ok(MsgExecuteContract {
            sender,
            contract: self.bridge.get_address_string(),
            msg: serde_json::to_vec(
                &msg::contracts::pyth_bridge::entry::ExecuteMsg::UpdatePrice {
                    market_id,
                    execs: None,
                    rewards: None,
                    bail_on_error: false,
                },
            )?,
            funds: vec![],
        })
    }
}
