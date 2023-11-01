use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDateTime, Utc};
use cosmos::Contract;
use cosmwasm_std::Uint256;
use msg::{
    contracts::market::{
        entry::OraclePriceResp,
        spot_price::{PythPriceServiceNetwork, SpotPriceConfig, SpotPriceFeed, SpotPriceFeedData},
    },
    prelude::*,
};
use perps_exes::pyth::fetch_json_with_retry;
use pyth_sdk_cw::PriceIdentifier;

use crate::app::App;

use super::markets::Market;

#[derive(Clone)]
pub struct PythOracle {
    pub contract: Contract,
    pub endpoint: String,
}

pub(crate) struct OffchainPriceData {
    /// Store the stable and edge values together since the IDs cannot overlap
    pub(crate) values: HashMap<PriceIdentifier, NonZero<Decimal256>>,
    pub(crate) stable_ids: HashSet<PriceIdentifier>,
    pub(crate) edge_ids: HashSet<PriceIdentifier>,
    /// The oldest publish time queried
    pub(crate) oldest_publish_time: Option<DateTime<Utc>>,
}

impl OffchainPriceData {
    #[tracing::instrument(skip_all)]
    pub(crate) async fn load(app: &App, markets: &[Market]) -> Result<OffchainPriceData> {
        // For now this is only Pyth data
        let mut stable_feeds = HashSet::new();
        let mut edge_feeds = HashSet::new();

        for market in markets {
            match &market.status.config.spot_price {
                SpotPriceConfig::Manual { .. } => (),
                SpotPriceConfig::Oracle {
                    pyth,
                    stride: _,
                    feeds,
                    feeds_usd,
                } => {
                    for SpotPriceFeed { data, inverted: _ } in feeds.iter().chain(feeds_usd.iter())
                    {
                        match data {
                            SpotPriceFeedData::Constant { .. } => (),
                            SpotPriceFeedData::Pyth {
                                id,
                                age_tolerance_seconds: _,
                            } => {
                                match pyth.as_ref().with_context(|| format!("Invalid config found, {} has a Pyth feed but not Pyth config", market.market_id))?.network {
                                PythPriceServiceNetwork::Stable => stable_feeds.insert(*id),
                                PythPriceServiceNetwork::Edge => edge_feeds.insert(*id)
                            };
                            }
                            SpotPriceFeedData::Stride { .. } => (),
                            SpotPriceFeedData::Sei { .. } => (),
                            SpotPriceFeedData::Simple { .. } => (),
                        }
                    }
                }
            }
        }

        let mut values = HashMap::new();
        let mut oldest_publish_time = None;
        fetch_pyth_prices(
            &app.client,
            &app.endpoint_stable,
            &stable_feeds,
            &mut values,
            &mut oldest_publish_time,
        )
        .await?;
        fetch_pyth_prices(
            &app.client,
            &app.endpoint_edge,
            &edge_feeds,
            &mut values,
            &mut oldest_publish_time,
        )
        .await?;

        Ok(OffchainPriceData {
            values,
            stable_ids: stable_feeds,
            edge_ids: edge_feeds,
            oldest_publish_time,
        })
    }
}

pub(crate) async fn get_latest_price(
    offchain_price_data: &OffchainPriceData,
    market: &Market,
) -> Result<(PriceBaseInQuote, PriceCollateralInUsd)> {
    match &market.status.config.spot_price {
        SpotPriceConfig::Manual { .. } => {
            bail!("Manual markets do not use an oracle")
        }
        SpotPriceConfig::Oracle {
            feeds, feeds_usd, ..
        } => {
            let oracle_price = market.market.get_oracle_price().await?;

            let base = compose_oracle_feeds(&oracle_price, &offchain_price_data.values, feeds)?;
            let base = PriceBaseInQuote::from_non_zero(base);

            let collateral =
                compose_oracle_feeds(&oracle_price, &offchain_price_data.values, feeds_usd)?;
            let collateral = PriceCollateralInUsd::from_non_zero(collateral);

            Ok((base, collateral))
        }
    }
}

fn compose_oracle_feeds(
    oracle_price: &OraclePriceResp,
    pyth_prices: &HashMap<PriceIdentifier, NumberGtZero>,
    feeds: &[SpotPriceFeed],
) -> Result<NumberGtZero> {
    let mut final_price = Decimal256::one();

    for feed in feeds {
        let component = match &feed.data {
            // pyth uses the latest-and-greatest from hermes, not the contract price
            SpotPriceFeedData::Pyth { id, .. } => pyth_prices
                .get(id)
                .with_context(|| format!("Missing pyth price for ID {}", id))?
                .into_decimal256(),
            SpotPriceFeedData::Constant { price } => price.into_decimal256(),
            SpotPriceFeedData::Sei { denom } => oracle_price
                .sei
                .get(denom)
                .with_context(|| format!("Missing price for Sei denom: {denom}"))?
                .into_decimal256(),
            SpotPriceFeedData::Stride { denom, .. } => {
                // we _could_ query the redemption rate from stride chain, but it's not needed
                // contract price is good enough
                oracle_price
                    .stride
                    .get(denom)
                    .with_context(|| format!("Missing redemption rate for Stride denom: {denom}"))?
                    .redemption_rate
                    .into_decimal256()
            }
            SpotPriceFeedData::Simple { contract, .. } => oracle_price
                .simple
                .get(&RawAddr::from(contract))
                .with_context(|| format!("Missing price for Simple contract: {contract}"))?
                .value
                .into_decimal256(),
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

#[tracing::instrument(skip_all)]
async fn fetch_pyth_prices(
    client: &reqwest::Client,
    endpoint: &str,
    ids: &HashSet<PriceIdentifier>,
    values: &mut HashMap<PriceIdentifier, NonZero<Decimal256>>,
    oldest_publish_time: &mut Option<DateTime<Utc>>,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct PythRecord {
        id: PriceIdentifier,
        price: PythPrice,
    }
    #[derive(serde::Deserialize)]
    struct PythPrice {
        expo: i8,
        price: Uint256,
        publish_time: i64,
    }

    if ids.is_empty() {
        return Ok(());
    }

    let base = format!("{}api/latest_price_feeds", endpoint);
    let records: Vec<PythRecord> = fetch_json_with_retry(|| {
        let mut req = client.get(&base);
        for id in ids {
            req = req.query(&[("ids[]", id)])
        }
        req
    })
    .await?;

    for PythRecord {
        id,
        price: PythPrice {
            expo,
            price,
            publish_time,
        },
    } in records
    {
        anyhow::ensure!(expo <= 0, "Exponent from Pyth must always be negative");
        let price = Decimal256::from_atomics(price, expo.abs().try_into()?)?;
        values.insert(
            id,
            NumberGtZero::try_from_decimal(price).with_context(|| {
                format!("unable to convert pyth price of {price} to NumberGtZero")
            })?,
        );

        match NaiveDateTime::from_timestamp_opt(publish_time, 0) {
            Some(publish_time) => {
                let publish_time = publish_time.and_utc();
                match *oldest_publish_time {
                    None => *oldest_publish_time = Some(publish_time),
                    Some(oldest) => {
                        if publish_time < oldest {
                            *oldest_publish_time = Some(publish_time);
                        }
                    }
                }
            }
            None => {
                tracing::error!("Could not convert Pyth publish time to NaiveDateTime, ignoring")
            }
        }
    }
    Ok(())
}
