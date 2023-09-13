use std::collections::HashMap;

use cosmos::{Contract, Cosmos, HasAddress};
use cosmwasm_std::Uint256;
use msg::{
    contracts::market::spot_price::{
        PythPriceServiceNetwork, SpotPriceConfig, SpotPriceFeed, SpotPriceFeedData,
    },
    prelude::*,
};
use perps_exes::pyth::VecWithCurr;

use super::markets::Market;

#[derive(Clone)]
pub(crate) struct Oracle {
    pub market_id: MarketId,
    pub spot_price_config: SpotPriceConfig,
    pub pyth: Option<PythOracle>,
}

#[derive(Clone)]
pub struct PythOracle {
    pub contract: Contract,
    pub endpoints: VecWithCurr<String>,
}

impl std::fmt::Debug for Oracle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_struct("Oracle");

        f.field("market_id", &self.market_id);
        if let Some(pyth) = &self.pyth {
            f.field("pyth_oracle_contract", &pyth.contract.get_address());
        }

        // TODO - add more debug info
        // .field("price_feed", &self.market_price_feeds.feeds)
        // .field(
        //     "price_feeds_usd",
        //     &format!("{:?}", self.market_price_feeds.feeds_usd),
        // )
        f.finish()
    }
}

impl Oracle {
    pub async fn new(
        cosmos: &Cosmos,
        market: &Market,
        pyth_endpoints_stable: VecWithCurr<String>,
        pyth_endpoints_edge: VecWithCurr<String>,
    ) -> Result<Self> {
        let market_id = market.market_id.clone();
        let status = market.market.status().await?;

        let spot_price_config = status.config.spot_price;

        let pyth = match &spot_price_config {
            SpotPriceConfig::Manual => None,
            SpotPriceConfig::Oracle { pyth, .. } => match pyth {
                None => None,
                Some(pyth) => {
                    let addr = pyth.contract_address.as_str().parse().with_context(|| {
                        format!(
                            "Invalid Pyth oracle contract from Config: {}",
                            pyth.contract_address
                        )
                    })?;
                    Some(PythOracle {
                        contract: cosmos.make_contract(addr),
                        endpoints: match pyth.network {
                            PythPriceServiceNetwork::Stable => pyth_endpoints_stable,
                            PythPriceServiceNetwork::Edge => pyth_endpoints_edge,
                        },
                    })
                }
            },
        };

        Ok(Self {
            pyth,
            market_id,
            spot_price_config,
        })
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

    pub async fn get_latest_price(
        &self,
        client: &reqwest::Client,
    ) -> Result<(PriceBaseInQuote, PriceCollateralInUsd)> {
        match &self.spot_price_config {
            SpotPriceConfig::Manual => {
                unimplemented!("FIXME: support manual oracle w/ contract query")
            }
            SpotPriceConfig::Oracle {
                feeds, feeds_usd, ..
            } => {
                let base = price_helper(client, self.pyth.as_ref(), feeds).await?;
                let base = PriceBaseInQuote::try_from_number(base.into_signed())?;

                let collateral = price_helper(client, self.pyth.as_ref(), feeds_usd).await?;
                let collateral = PriceCollateralInUsd::try_from_number(collateral.into_signed())?;

                Ok((base, collateral))
            }
        }
    }
}

// FIXME: move out of pyth, handle more than just pyth
async fn price_helper(
    client: &reqwest::Client,
    pyth: Option<&PythOracle>,
    feeds: &[SpotPriceFeed],
) -> Result<Decimal256> {
    #[derive(serde::Deserialize)]
    struct PythRecord {
        id: String,
        price: PythPrice,
    }
    #[derive(serde::Deserialize)]
    struct PythPrice {
        expo: i8,
        price: Uint256,
    }

    let pyth_prices = match pyth {
        None => HashMap::new(),
        Some(pyth) => {
            pyth.endpoints
                .try_any_from_curr_async(|endpoint| async move {
                    let mut req = client.get(format!("{}api/latest_price_feeds", endpoint));
                    for feed in feeds {
                        if let SpotPriceFeedData::Pyth { id, .. } = feed.data {
                            req = req.query(&[("ids[]", id)]);
                        }
                    }

                    let records = req
                        .send()
                        .await?
                        .error_for_status()?
                        .json::<Vec<PythRecord>>()
                        .await?;

                    Ok(records
                        .into_iter()
                        .map(|PythRecord { id, price }| (id, price))
                        .collect::<HashMap<_, _>>())
                })
                .await?
        }
    };

    let mut final_price = Decimal256::one();

    for feed in feeds {
        let component = match feed.data {
            SpotPriceFeedData::Pyth { id, .. } => {
                let PythPrice { expo, price } = pyth_prices
                    .get(&id.to_hex())
                    .with_context(|| format!("Missing price for ID {}", id))?;

                anyhow::ensure!(*expo <= 0, "Exponent from Pyth must always be negative");
                Decimal256::from_atomics(*price, expo.abs().try_into()?)?
            }
            SpotPriceFeedData::Constant { price } => price.into_decimal256(),
            SpotPriceFeedData::Sei { .. } => {
                unimplemented!("FIXME: reach out to sei oracle");
            }
            SpotPriceFeedData::Stride { .. } => {
                unimplemented!("FIXME: reach out to stride");
            }
        };

        if feed.inverted {
            final_price = final_price.checked_div(component)?;
        } else {
            final_price = final_price.checked_mul(component)?;
        }
    }

    Ok(final_price)
}
