use std::{collections::HashMap, sync::Arc};

use crate::app::PythEndpoints;
use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Address, Contract, Cosmos, HasAddress,
};
use cosmwasm_std::{Binary, Coin, Uint256};
use msg::{
    contracts::pyth_bridge::{
        entry::QueryMsg as BridgeQueryMsg, MarketPrice, PythMarketPriceFeeds, PythPriceFeed,
    },
    prelude::*,
};
use perps_exes::config::PythConfig;
use pyth_sdk_cw::PriceIdentifier;

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
                    bail_on_error: false,
                },
            )?,
            funds: vec![],
        })
    }

    pub async fn get_wormhole_proofs(
        &self,
        client: &reqwest::Client,
        endpoints: &PythEndpoints,
    ) -> Result<Vec<String>> {
        let mut all_ids: Vec<PriceIdentifier> =
            self.market_price_feeds.feeds.iter().map(|f| f.id).collect();
        if let Some(feeds_usd) = &self.market_price_feeds.feeds_usd {
            all_ids.extend(feeds_usd.iter().map(|f| f.id));
        }

        all_ids.sort();
        all_ids.dedup();
        let all_ids_len = all_ids.len();
        // pyth uses this format for array params: https://github.com/axios/axios/blob/9588fcdec8aca45c3ba2f7968988a5d03f23168c/test/specs/helpers/buildURL.spec.js#L31
        let url_params = all_ids
            .iter()
            .map(|id| format!("ids[]={id}"))
            .collect::<Vec<String>>()
            .join("&");
        let url_params = &url_params;

        let mut endpoints = endpoints.clone();
        Arc::get_mut(&mut endpoints)
            .expect("Unable to get_mut on endpoints!")
            .try_any_from_curr_async(|endpoint| async move {
                let url = format!("{endpoint}api/latest_vaas?{url_params}");

                let vaas: Vec<String> = client
                    .get(url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;

                if vaas.len() != all_ids_len {
                    anyhow::bail!("expected {} vaas, got {}", all_ids_len, vaas.len());
                }
                Ok(vaas)
            })
            .await
    }
}

/// Get the latest price from Pyth
pub async fn get_latest_price(
    client: &reqwest::Client,
    market_price_feeds: &PythMarketPriceFeeds,
) -> Result<(PriceBaseInQuote, Option<PriceCollateralInUsd>)> {
    let pyth_config = PythConfig::load()?;
    let mut endpoints = crate::util::helpers::VecWithCurr::new(pyth_config.endpoints.iter());
    endpoints
        .try_any_from_curr_async(|endpoint| async move {
            let base = price_helper(client, &endpoint, &market_price_feeds.feeds).await?;
            let base = PriceBaseInQuote::try_from_number(base.into_signed())?;

            let collateral = match &market_price_feeds.feeds_usd {
                Some(feeds_usd) => {
                    let collateral = price_helper(client, &endpoint, feeds_usd).await?;
                    Some(PriceCollateralInUsd::try_from_number(
                        collateral.into_signed(),
                    ))
                }
                None => None,
            }
            .transpose()?;

            Ok((base, collateral))
        })
        .await
}

async fn price_helper(
    client: &reqwest::Client,
    endpoint: &str,
    feeds: &[PythPriceFeed],
) -> Result<Decimal256> {
    let mut req = client.get(format!("{}api/latest_price_feeds", endpoint));
    for feed in feeds {
        req = req.query(&[("ids[]", feed.id)]);
    }

    #[derive(serde::Deserialize)]
    struct Record {
        id: String,
        price: Price,
    }
    #[derive(serde::Deserialize)]
    struct Price {
        expo: i8,
        price: Uint256,
    }
    let records = req
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Record>>()
        .await?;

    let prices = records
        .into_iter()
        .map(|Record { id, price }| (id, price))
        .collect::<HashMap<_, _>>();

    let mut final_price = Decimal256::one();

    for feed in feeds {
        let Price { expo, price } = prices
            .get(&feed.id.to_hex())
            .with_context(|| format!("Missing price for ID {}", feed.id))?;

        anyhow::ensure!(*expo <= 0, "Exponent from Pyth must always be negative");
        let component = Decimal256::from_atomics(*price, expo.abs().try_into()?)?;
        if feed.inverted {
            final_price = final_price.checked_div(component)?;
        } else {
            final_price = final_price.checked_mul(component)?;
        }
    }

    Ok(final_price)
}
