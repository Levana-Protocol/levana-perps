use std::collections::{HashMap, HashSet};

use cosmos::{Contract, Cosmos, HasAddress};
use cosmwasm_std::Uint256;
use msg::{
    contracts::market::{
        entry::OraclePriceResp,
        spot_price::{PythPriceServiceNetwork, SpotPriceConfig, SpotPriceFeed, SpotPriceFeedData},
    },
    prelude::*,
};

use super::markets::Market;

#[derive(Clone)]
pub(crate) struct Oracle {
    pub market: Market,
    pub spot_price_config: SpotPriceConfig,
    pub pyth: Option<PythOracle>,
}

#[derive(Clone)]
pub struct PythOracle {
    pub contract: Contract,
    pub endpoint: String,
}

impl std::fmt::Debug for Oracle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_struct("Oracle");

        f.field("market_id", &self.market.market_id);
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
        market: Market,
        pyth_endpoint_stable: impl Into<String>,
        pyth_endpoint_edge: impl Into<String>,
    ) -> Result<Self> {
        let status = market.market.status().await?;

        let spot_price_config = status.config.spot_price;

        let pyth = match &spot_price_config {
            SpotPriceConfig::Manual { .. } => None,
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
                        endpoint: match pyth.network {
                            PythPriceServiceNetwork::Stable => pyth_endpoint_stable.into(),
                            PythPriceServiceNetwork::Edge => pyth_endpoint_edge.into(),
                        },
                    })
                }
            },
        };

        Ok(Self {
            pyth,
            market,
            spot_price_config,
        })
    }

    pub async fn get_latest_price(
        &self,
        client: &reqwest::Client,
    ) -> Result<(PriceBaseInQuote, PriceCollateralInUsd)> {
        match &self.spot_price_config {
            SpotPriceConfig::Manual { .. } => {
                bail!("Manual markets do not use an oracle")
            }
            SpotPriceConfig::Oracle {
                feeds, feeds_usd, ..
            } => {
                let pyth_prices = match &self.pyth {
                    None => HashMap::new(),
                    Some(pyth) => {
                        fetch_pyth_prices(client, pyth, feeds.iter().chain(feeds_usd.iter()))
                            .await?
                    }
                };

                let oracle_price = self.market.market.get_oracle_price().await?;

                let base = compose_oracle_feeds(&oracle_price, &pyth_prices, feeds)?;
                let base = PriceBaseInQuote::from_non_zero(base);

                let collateral = compose_oracle_feeds(&oracle_price, &pyth_prices, feeds_usd)?;
                let collateral = PriceCollateralInUsd::from_non_zero(collateral);

                Ok((base, collateral))
            }
        }
    }
}

pub fn compose_oracle_feeds(
    oracle_price: &OraclePriceResp,
    pyth_prices: &HashMap<String, NumberGtZero>,
    feeds: &[SpotPriceFeed],
) -> Result<NumberGtZero> {
    let mut final_price = Decimal256::one();

    for feed in feeds {
        let component = match &feed.data {
            // pyth uses the latest-and-greatest from hermes, not the contract price
            SpotPriceFeedData::Pyth { id, .. } => pyth_prices
                .get(&id.to_hex())
                .with_context(|| format!("Missing pyth price for ID {}", id))?
                .into_decimal256(),
            SpotPriceFeedData::Constant { price } => price.into_decimal256(),
            SpotPriceFeedData::Sei { denom } => oracle_price
                .sei
                .get(denom)
                .with_context(|| format!("Missing price for Sei denom: {denom}"))?
                .into_decimal256(),
            SpotPriceFeedData::Stride { denom } => {
                let redemption_value = oracle_price
                    .stride
                    .get(denom)
                    .with_context(|| format!("Missing redemption price for Stride denom: {denom}"))?
                    .into_decimal256();

                unimplemented!("FIXME: use stride redemption value of {redemption_value}");
            }
        };

        if feed.inverted {
            final_price = final_price.checked_div(component)?;
        } else {
            final_price = final_price.checked_mul(component)?;
        }
    }

    NumberGtZero::try_from_decimal(final_price)
        .with_context(|| format!("unable to convert price of {final_price} to NumberGtZero"))
}

async fn fetch_pyth_prices(
    client: &reqwest::Client,
    pyth: &PythOracle,
    feeds: impl Iterator<Item = &SpotPriceFeed>,
) -> Result<HashMap<String, NumberGtZero>> {
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

    let mut req = client.get(format!("{}api/latest_price_feeds", pyth.endpoint));

    let mut ids = HashSet::new();

    for feed in feeds {
        if let SpotPriceFeedData::Pyth { id, .. } = feed.data {
            // only fetch unique ids
            if !ids.contains(&id) {
                req = req.query(&[("ids[]", id)]);
                ids.insert(id);
            }
        }
    }

    if !ids.is_empty() {
        let records = req
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<PythRecord>>()
            .await?;

        let mut output = HashMap::new();
        for PythRecord {
            id,
            price: PythPrice { expo, price },
        } in records
        {
            anyhow::ensure!(expo <= 0, "Exponent from Pyth must always be negative");
            let price = Decimal256::from_atomics(price, expo.abs().try_into()?)?;
            output.insert(
                id,
                NumberGtZero::try_from_decimal(price).with_context(|| {
                    format!("unable to convert pyth price of {price} to NumberGtZero")
                })?,
            );
        }
        Ok(output)
    } else {
        Ok(HashMap::new())
    }
}
