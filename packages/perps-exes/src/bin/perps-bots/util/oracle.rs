use std::collections::{HashMap, hash_map::Entry};

use cosmos::{Contract, Cosmos, HasAddress};
use cosmwasm_std::Uint256;
use msg::{
    contracts::market::{spot_price::{
        PythPriceServiceNetwork, SpotPriceConfig, SpotPriceFeed, SpotPriceFeedData,
    }, entry::{OraclePriceResp, OraclePriceFeedResp}},
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
                let oracle_price:SanitizedOraclePrice = self.market.market.get_oracle_price().await?.try_into()?;

                let base = price_helper(client, self.pyth.as_ref(), &oracle_price, feeds).await?;
                let base = PriceBaseInQuote::from_non_zero(base);

                let collateral = price_helper(client, self.pyth.as_ref(), &oracle_price, feeds_usd).await?;
                let collateral = PriceCollateralInUsd::from_non_zero(collateral);

                Ok((base, collateral))
            }
        }
    }
}

// oracle prices may be duplicated and/or inverted, so we need to sanitize first
struct SanitizedOraclePrice {
    // sei denom -> price
    pub sei: HashMap<String, NumberGtZero>,
    // stride denom -> redemption price
    pub stride: HashMap<String, NumberGtZero>,
}

impl TryFrom<OraclePriceResp> for SanitizedOraclePrice {
    type Error = anyhow::Error;

    fn try_from(oracle_price: OraclePriceResp) -> Result<Self, Self::Error> {
        let mut sei = HashMap::new();
        let mut stride = HashMap::new();

        let all_feeds_iter = oracle_price.feeds.iter().chain(oracle_price.feeds_usd.iter());

        for OraclePriceFeedResp { feed, price, publish_time: _ } in all_feeds_iter {
            match &feed.data {
                SpotPriceFeedData::Sei { denom } => {
                    if let Entry::Vacant(entry) = sei.entry(denom.clone()) {
                        entry.insert(if feed.inverted {
                            Number::ONE
                                .checked_div(price.into_number())
                                .and_then(|x| NumberGtZero::try_from(x))
                                .with_context(|| format!("unable to invert sei price of {price} for {denom}"))?
                        } else {
                            *price
                        });
                    }
                }
                SpotPriceFeedData::Stride { denom } => {
                    if let Entry::Vacant(entry) = stride.entry(denom.clone()) {
                        entry.insert(if feed.inverted {
                            Number::ONE
                                .checked_div(price.into_number())
                                .and_then(|x| NumberGtZero::try_from(x))
                                .with_context(|| format!("unable to invert stride redemption price of {price} for {denom}"))?
                        } else {
                            *price
                        });
                    }
                },
                SpotPriceFeedData::Constant { .. } => {
                    // ignore constants, they are mixed in price_helper directly
                }
                SpotPriceFeedData::Pyth { .. } => {
                    // ignore pyth, they are mixed in via hermes fetching 
                }
            }
        }

        Ok(Self { sei, stride })
    }
}

async fn price_helper(
    client: &reqwest::Client,
    pyth: Option<&PythOracle>,
    oracle_price: &SanitizedOraclePrice,
    feeds: &[SpotPriceFeed],
) -> Result<NumberGtZero> {
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

    // pyth prices come from latest-and-greatest hermes
    let pyth_prices = match pyth {
        None => HashMap::new(),
        Some(pyth) => {
            let mut req = client.get(format!("{}api/latest_price_feeds", pyth.endpoint));
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

            records
                .into_iter()
                .map(|PythRecord { id, price }| (id, price))
                .collect::<HashMap<_, _>>()
        }
    };

    // other oracle prices come from third-party contracts, native chain oracle, etc.
    // and need to be mixed in with the pyth prices

    let mut final_price = Decimal256::one();

    for feed in feeds {
        let component = match &feed.data {
            SpotPriceFeedData::Pyth { id, .. } => {
                let PythPrice { expo, price } = pyth_prices
                    .get(&id.to_hex())
                    .with_context(|| format!("Missing price for ID {}", id))?;

                anyhow::ensure!(*expo <= 0, "Exponent from Pyth must always be negative");
                Decimal256::from_atomics(*price, expo.abs().try_into()?)?
            }
            SpotPriceFeedData::Constant { price } => price.into_decimal256(),
            SpotPriceFeedData::Sei { denom } => {
                oracle_price
                    .sei
                    .get(denom)
                    .with_context(|| format!("Missing price for Sei denom: {denom}"))?
                    .into_decimal256()
            }
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

    NumberGtZero::try_from_decimal(final_price).with_context(|| format!("unable to convert price of {final_price} to NumberGtZero"))
}
