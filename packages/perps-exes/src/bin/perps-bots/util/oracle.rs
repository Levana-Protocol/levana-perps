use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDateTime, Utc};
use cosmos::{Address, Contract};
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

#[derive(Clone)]
pub(crate) struct OffchainPriceData {
    /// Store the stable and edge values together since the IDs cannot overlap
    pub(crate) values: HashMap<PriceIdentifier, (NonZero<Decimal256>, DateTime<Utc>)>,
    pub(crate) stable_ids: HashSet<PriceIdentifier>,
    pub(crate) edge_ids: HashSet<PriceIdentifier>,
}

impl OffchainPriceData {
    #[tracing::instrument(skip_all)]
    pub(crate) async fn load(app: &App, markets: &[Market]) -> Result<OffchainPriceData> {
        // For now this is only Pyth data
        let mut stable_feeds = HashSet::new();
        let mut edge_feeds = HashSet::new();

        for market in markets {
            match &market.config.spot_price {
                SpotPriceConfig::Manual { .. } => (),
                SpotPriceConfig::Oracle {
                    pyth,
                    stride: _,
                    feeds,
                    feeds_usd,
                    volatile_diff_seconds: _,
                } => {
                    for SpotPriceFeed {
                        data,
                        inverted: _,
                        volatile: _,
                    } in feeds.iter().chain(feeds_usd.iter())
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
        })
    }
}

pub(crate) enum LatestPrice {
    NoPriceInContract,
    PricesFound {
        /// Price calculated from combination of on-chain and off-chain data sources
        off_chain_price: PriceBaseInQuote,
        /// Publish time calculated from on-chain and off-chain data sources
        off_chain_publish_time: DateTime<Utc>,
        /// Price calculated from latest on-chain oracle data
        on_chain_oracle_price: PriceBaseInQuote,
        /// Publish time calculated from on-chain oracle data
        on_chain_oracle_publish_time: DateTime<Utc>,
    },
    PriceTooOld {
        too_old: PriceTooOld,
    },
    VolatileDiffTooLarge,
}

/// Get the latest off-chain price point
pub(crate) async fn get_latest_price(
    offchain_price_data: &OffchainPriceData,
    market: &Market,
) -> Result<LatestPrice> {
    let (feeds, volatile_diff_seconds) = match &market.config.spot_price {
        SpotPriceConfig::Manual { .. } => {
            bail!("Manual markets do not use an oracle")
        }
        SpotPriceConfig::Oracle {
            feeds,
            volatile_diff_seconds,
            ..
        } => (feeds, volatile_diff_seconds.unwrap_or(5)),
    };

    let oracle_price = match market.market.get_oracle_price(false).await {
        Ok(oracle_price) => oracle_price,
        Err(e) => {
            return if format!("{e:?}").contains("no_price_found") {
                Ok(LatestPrice::NoPriceInContract)
            } else {
                Err(e.into())
            }
        }
    };

    Ok(
        match compose_oracle_feeds(
            &oracle_price,
            &offchain_price_data.values,
            feeds,
            volatile_diff_seconds,
        )? {
            ComposedOracleFeed::UpdateTooOld { too_old } => LatestPrice::PriceTooOld { too_old },
            ComposedOracleFeed::VolatileDiffTooLarge => LatestPrice::VolatileDiffTooLarge,
            ComposedOracleFeed::OffChainPrice {
                price: off_chain_price,
                publish_time: off_chain_publish_time,
            } => LatestPrice::PricesFound {
                off_chain_price,
                off_chain_publish_time,
                on_chain_oracle_price: oracle_price.composed_price.price_base,
                on_chain_oracle_publish_time: oracle_price
                    .composed_price
                    .timestamp
                    .try_into_chrono_datetime()?,
            },
        },
    )
}

#[derive(Debug)]
pub(crate) struct PriceTooOld {
    pub(crate) feed: FeedType,
    pub(crate) check_time: DateTime<Utc>,
    pub(crate) publish_time: DateTime<Utc>,
    pub(crate) age: i64,
    pub(crate) age_tolerance_seconds: u32,
}

enum ComposedOracleFeed {
    UpdateTooOld {
        too_old: PriceTooOld,
    },
    OffChainPrice {
        price: PriceBaseInQuote,
        publish_time: DateTime<Utc>,
    },
    VolatileDiffTooLarge,
}

#[derive(Debug)]
pub(crate) enum FeedType {
    Pyth { id: PriceIdentifier },
    Stride { denom: String },
    Simple { contract: Address },
}

impl Display for FeedType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FeedType::Pyth { id } => write!(f, "Pyth feed {id}"),
            FeedType::Stride { denom } => write!(f, "Stride denom {denom}"),
            FeedType::Simple { contract } => write!(f, "Simple contract {contract}"),
        }
    }
}

fn compose_oracle_feeds(
    oracle_price: &OraclePriceResp,
    pyth_prices: &HashMap<PriceIdentifier, (NumberGtZero, DateTime<Utc>)>,
    feeds: &[SpotPriceFeed],
    volatile_diff_seconds: u32,
) -> Result<ComposedOracleFeed> {
    let mut final_price = Decimal256::one();
    let mut publish_times = None::<(DateTime<Utc>, DateTime<Utc>)>;
    let now = Utc::now();

    let mut update_publish_time =
        |new_time: DateTime<Utc>, is_volatile_opt: Option<bool>, is_volatile_default: bool| {
            if is_volatile_opt.unwrap_or(is_volatile_default) {
                publish_times = Some(match publish_times {
                    Some((oldest, newest)) => (oldest.min(new_time), newest.max(new_time)),
                    None => (new_time, new_time),
                });
            }
        };

    for feed in feeds {
        let component = match &feed.data {
            // pyth uses the latest-and-greatest from hermes, not the contract price
            SpotPriceFeedData::Pyth {
                id,
                age_tolerance_seconds,
            } => {
                let (price, pyth_update) = pyth_prices
                    .get(id)
                    .with_context(|| format!("Missing pyth price for ID {}", id))?;
                let age = now.signed_duration_since(pyth_update).num_seconds();
                if age >= (*age_tolerance_seconds).into() {
                    return Ok(ComposedOracleFeed::UpdateTooOld {
                        too_old: PriceTooOld {
                            feed: FeedType::Pyth { id: *id },
                            check_time: now,
                            publish_time: *pyth_update,
                            age,
                            age_tolerance_seconds: *age_tolerance_seconds,
                        },
                    });
                }

                update_publish_time(*pyth_update, feed.volatile, true);
                price.into_decimal256()
            }
            SpotPriceFeedData::Constant { price } => {
                anyhow::ensure!(
                    !feed.volatile.unwrap_or(false),
                    "Constant feeds cannot be volatile"
                );
                price.into_decimal256()
            }
            SpotPriceFeedData::Sei { denom } => {
                let sei = oracle_price
                    .sei
                    .get(denom)
                    .with_context(|| format!("Missing price for Sei denom: {denom}"))?;
                update_publish_time(
                    sei.publish_time.try_into_chrono_datetime()?,
                    feed.volatile,
                    true,
                );
                sei.price.into_decimal256()
            }
            SpotPriceFeedData::Stride {
                denom,
                age_tolerance_seconds,
            } => {
                // we _could_ query the redemption rate from stride chain, but it's not needed
                // contract price is good enough
                let stride = oracle_price.stride.get(denom).with_context(|| {
                    format!("Missing redemption rate for Stride denom: {denom}")
                })?;
                let publish_time = stride.publish_time.try_into_chrono_datetime()?;
                let age = now.signed_duration_since(publish_time).num_seconds();
                if age >= (*age_tolerance_seconds).into() {
                    return Ok(ComposedOracleFeed::UpdateTooOld {
                        too_old: PriceTooOld {
                            feed: FeedType::Stride {
                                denom: denom.clone(),
                            },
                            check_time: now,
                            publish_time,
                            age,
                            age_tolerance_seconds: *age_tolerance_seconds,
                        },
                    });
                }
                update_publish_time(publish_time, feed.volatile, false);
                stride.redemption_rate.into_decimal256()
            }
            SpotPriceFeedData::Simple {
                contract,
                age_tolerance_seconds,
            } => {
                let simple = oracle_price
                    .simple
                    .get(&RawAddr::from(contract))
                    .with_context(|| format!("Missing price for Simple contract: {contract}"))?;
                if let Some(timestamp) = simple.timestamp {
                    let timestamp = timestamp.try_into_chrono_datetime()?;
                    let age = now.signed_duration_since(timestamp).num_seconds();
                    if age >= (*age_tolerance_seconds).into() {
                        return Ok(ComposedOracleFeed::UpdateTooOld {
                            too_old: PriceTooOld {
                                feed: FeedType::Simple {
                                    contract: contract.as_str().parse()?,
                                },
                                check_time: now,
                                publish_time: timestamp,
                                age,
                                age_tolerance_seconds: *age_tolerance_seconds,
                            },
                        });
                    }
                    update_publish_time(timestamp, feed.volatile, false);
                }
                simple.value.into_decimal256()
            }
        };

        if feed.inverted {
            final_price = final_price.checked_div(component)?;
        } else {
            final_price = final_price.checked_mul(component)?;
        }
    }

    let price = NumberGtZero::try_from_decimal(final_price)
        .with_context(|| format!("unable to convert price of {final_price} to NumberGtZero"))?;

    let (oldest, newest) =
        publish_times.context("No publish time found while composing oracle price")?;
    let diff = newest.signed_duration_since(oldest);
    Ok(if diff.num_seconds() > volatile_diff_seconds.into() {
        ComposedOracleFeed::VolatileDiffTooLarge
    } else {
        ComposedOracleFeed::OffChainPrice {
            price: PriceBaseInQuote::from_non_zero(price),
            publish_time: oldest,
        }
    })
}

#[tracing::instrument(skip_all)]
async fn fetch_pyth_prices(
    client: &reqwest::Client,
    endpoint: &str,
    ids: &HashSet<PriceIdentifier>,
    values: &mut HashMap<PriceIdentifier, (NonZero<Decimal256>, DateTime<Utc>)>,
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
        let publish_time = match NaiveDateTime::from_timestamp_opt(publish_time, 0) {
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
                publish_time
            }
            None => {
                tracing::error!("Could not convert Pyth publish time to NaiveDateTime, ignoring");
                Utc::now()
            }
        };
        anyhow::ensure!(expo <= 0, "Exponent from Pyth must always be negative");
        let price = Decimal256::from_atomics(price, expo.abs().try_into()?)?;
        let price = NumberGtZero::try_from_decimal(price)
            .with_context(|| format!("unable to convert pyth price of {price} to NumberGtZero"))?;
        values.insert(id, (price, publish_time));
    }
    Ok(())
}
