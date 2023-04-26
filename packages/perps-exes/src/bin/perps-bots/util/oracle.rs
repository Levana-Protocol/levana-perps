use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Address, Contract, Cosmos, HasAddress,
};
use cosmwasm_std::{Binary, Coin};
use msg::{
    contracts::pyth_bridge::{
        entry::QueryMsg as BridgeQueryMsg, MarketPrice, PythMarketPriceFeeds,
    },
    prelude::*,
};
use perps_exes::config::DeploymentConfig;
use pyth_sdk_cw::PriceIdentifier;

#[derive(Clone)]
pub(crate) struct Pyth {
    pub endpoint: String,
    pub oracle: Contract,
    pub bridge: Contract,
    pub market_id: MarketId,
    pub market_price_feeds: PythMarketPriceFeeds,
}

impl std::fmt::Debug for Pyth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pyth")
            .field("endpoint", &self.endpoint)
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
    pub async fn new(
        cosmos: &Cosmos,
        config: &DeploymentConfig,
        bridge_addr: Address,
        market_id: MarketId,
    ) -> Result<Self> {
        let endpoint = config
            .pyth
            .as_ref()
            .context("must have a pyth endpoint if there's a pyth bridge")?
            .endpoint
            .clone();
        let bridge = cosmos.make_contract(bridge_addr);
        let oracle_addr = bridge.query(BridgeQueryMsg::PythAddress {}).await?;
        let oracle = cosmos.make_contract(oracle_addr);
        let market_price_feeds = bridge
            .query(BridgeQueryMsg::MarketPriceFeeds {
                market_id: market_id.clone(),
            })
            .await?;

        Ok(Self {
            endpoint,
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

    pub async fn get_oracle_update_msg(
        &self,
        sender: String,
        vaas: Vec<String>,
    ) -> Result<MsgExecuteContract> {
        let vaas_binary = vaas
            .iter()
            .map(|vaa| Binary::from_base64(vaa).map_err(|err| err.into()))
            .collect::<Result<Vec<_>>>()?;

        let fees: Coin = self
            .oracle
            .query(pyth_sdk_cw::QueryMsg::GetUpdateFee {
                vaas: vaas_binary.clone(),
            })
            .await?;

        Ok(MsgExecuteContract {
            sender,
            contract: self.oracle.get_address_string(),
            msg: serde_json::to_vec(&pyth_sdk_cw::ExecuteMsg::UpdatePriceFeeds {
                data: vaas_binary,
            })?,
            funds: vec![cosmos::Coin {
                denom: fees.denom,
                amount: fees.amount.to_string(),
            }],
        })
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
                },
            )?,
            funds: vec![],
        })
    }

    pub async fn get_wormhole_proofs(&self, client: &reqwest::Client) -> Result<Vec<String>> {
        let mut all_ids: Vec<PriceIdentifier> =
            self.market_price_feeds.feeds.iter().map(|f| f.id).collect();
        if let Some(feeds_usd) = &self.market_price_feeds.feeds_usd {
            all_ids.extend(feeds_usd.iter().map(|f| f.id));
        }

        all_ids.sort();
        all_ids.dedup();

        let mut url = format!("{}api/latest_vaas", self.endpoint);

        for (index, id) in all_ids.iter().enumerate() {
            // pyth uses this format for array params: https://github.com/axios/axios/blob/9588fcdec8aca45c3ba2f7968988a5d03f23168c/test/specs/helpers/buildURL.spec.js#L31
            let delim = if index == 0 { "?" } else { "&" };
            url.push_str(&format!("{}ids[]={}", delim, id));
        }

        let vaas: Vec<String> = client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if vaas.len() != all_ids.len() {
            anyhow::bail!("expected {} vaas, got {}", all_ids.len(), vaas.len());
        }

        Ok(vaas)
    }
}
