use std::collections::{hash_map::Entry, HashMap, HashSet};

use chrono::{DateTime, Utc};
use cosmos::{Address, Contract};
use cosmwasm_std::Uint256;
use msg::{
    contracts::market::{
        entry::OraclePriceResp,
        spot_price::{PythPriceServiceNetwork, SpotPriceConfig, SpotPriceFeed, SpotPriceFeedData},
    },
    prelude::*,
};
use parking_lot::RwLock;
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
            &app.pyth_stats,
        )
        .await?;
        fetch_pyth_prices(
            &app.client,
            &app.endpoint_edge,
            &edge_feeds,
            &mut values,
            &mut oldest_publish_time,
            &app.pyth_stats,
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
        /// Current on-chain price point
        on_chain_price_point: PricePoint,
        /// Did any of the feeds indicate that a Pyth update was needed?
        requires_pyth_update: bool,
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
    let on_chain_price_point = match market.market.current_price().await {
        Ok(price_point) => price_point,
        Err(e) => {
            return if e.to_string().contains("price_not_found") {
                Ok(LatestPrice::NoPriceInContract)
            } else {
                Err(e.into())
            };
        }
    };
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
                requires_pyth_update,
            } => LatestPrice::PricesFound {
                off_chain_price,
                off_chain_publish_time,
                on_chain_oracle_price: oracle_price.composed_price.price_base,
                on_chain_oracle_publish_time: oracle_price
                    .composed_price
                    .timestamp
                    .try_into_chrono_datetime()?,
                on_chain_price_point,
                requires_pyth_update,
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
        requires_pyth_update: bool,
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
    offchain_pyth_prices: &HashMap<PriceIdentifier, (NumberGtZero, DateTime<Utc>)>,
    feeds: &[SpotPriceFeed],
    volatile_diff_seconds: u32,
) -> Result<ComposedOracleFeed> {
    let mut final_price = Decimal256::one();
    let mut publish_times = None::<(DateTime<Utc>, DateTime<Utc>)>;
    let now = Utc::now();
    let mut requires_pyth_update = false;

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
                // We perform two age checks. First: make sure that the
                // off-chain price is new enough to satisfy age tolerance. If not, we don't want to
                // interact with this market at all.
                let (price, off_chain_pyth_update) = offchain_pyth_prices
                    .get(id)
                    .with_context(|| format!("Missing pyth price for ID {}", id))?;
                let off_chain_pyth_update = *off_chain_pyth_update;

                let off_chain_age = now
                    .signed_duration_since(off_chain_pyth_update)
                    .num_seconds();
                let age_tolerance_seconds = *age_tolerance_seconds;
                if off_chain_age >= age_tolerance_seconds.into() {
                    return Ok(ComposedOracleFeed::UpdateTooOld {
                        too_old: PriceTooOld {
                            feed: FeedType::Pyth { id: *id },
                            check_time: now,
                            publish_time: off_chain_pyth_update,
                            age: off_chain_age,
                            age_tolerance_seconds,
                        },
                    });
                }
                update_publish_time(off_chain_pyth_update, feed.volatile, true);

                // Now that we know we have a recent enough off-chain price,
                // check if the on-chain price is too old. If it is, indicate that we need to do a
                // price update in order to crank.
                if let Some(on_chain_pyth_update) = oracle_price.pyth.get(id) {
                    let on_chain_pyth_update = on_chain_pyth_update
                        .publish_time
                        .try_into_chrono_datetime()?;
                    let age = now
                        .signed_duration_since(on_chain_pyth_update)
                        .num_seconds();
                    // Add (arbitrarily) 3 seconds to the age to minimize cases
                    // of submitting a transaction that lands after the age tolerance fails.
                    let age = age + 3;
                    if age >= age_tolerance_seconds.into() {
                        requires_pyth_update = true;
                    }
                }

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
            requires_pyth_update,
        }
    })
}

/// Statistics on the age of Pyth prices when queried.
#[derive(Default)]
pub(crate) struct PythPriceStats {
    pub(crate) feeds: RwLock<HashMap<PriceIdentifier, PythPriceStatsSingle>>,
}
impl PythPriceStats {
    pub(crate) fn get_status(&self) -> Vec<String> {
        let mut lines = vec![];
        for (
            id,
            PythPriceStatsSingle {
                last_age,
                total_ages,
                count_ages,
            },
        ) in self.feeds.read().iter()
        {
            assert!(*count_ages > 0);
            let average = Decimal256::from_ratio(*total_ages, *count_ages);
            lines.push(format!(
                "{id}: last age {last_age}, average age {average}, data points: {count_ages}"
            ));
        }
        lines.sort();
        lines
    }
}

#[derive(Clone)]
pub(crate) struct PythPriceStatsSingle {
    pub(crate) last_age: u64,
    pub(crate) total_ages: u64,
    pub(crate) count_ages: u64,
}

impl PythPriceStatsSingle {
    fn new(age: u64) -> Self {
        Self {
            last_age: age,
            total_ages: age,
            count_ages: 1,
        }
    }

    fn add_age(&mut self, age: u64) {
        self.last_age = age;
        self.total_ages += age;
        self.count_ages += 1;
    }
}

#[tracing::instrument(skip_all)]
async fn fetch_pyth_prices(
    client: &reqwest::Client,
    endpoint: &reqwest::Url,
    ids: &HashSet<PriceIdentifier>,
    values: &mut HashMap<PriceIdentifier, (NonZero<Decimal256>, DateTime<Utc>)>,
    oldest_publish_time: &mut Option<DateTime<Utc>>,
    stats: &PythPriceStats,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct PythPriceResponse {
        parsed: Vec<PythRecord>,
    }

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

    let base = endpoint.join("v2/updates/price/latest")?;
    let ids_iter = ids.iter().map(|feed| ("ids[]", feed.to_hex()));
    let ids_iter = ids_iter.chain([("parsed", "true".to_owned())]);
    let url = reqwest::Url::parse_with_params(base.as_str(), ids_iter)?;

    let records: PythPriceResponse = fetch_json_with_retry(|| client.get(url.clone())).await?;
    let records = records.parsed;

    let now = Utc::now();
    for PythRecord {
        id,
        price: PythPrice {
            expo,
            price,
            publish_time,
        },
    } in records
    {
        let publish_time = match DateTime::from_timestamp(publish_time, 0) {
            Some(publish_time) => {
                match *oldest_publish_time {
                    None => *oldest_publish_time = Some(publish_time),
                    Some(oldest) => {
                        if publish_time < oldest {
                            *oldest_publish_time = Some(publish_time);
                        }
                    }
                }
                let age = now - publish_time;
                if let Ok(age) = u64::try_from(age.num_seconds()) {
                    match stats.feeds.write().entry(id) {
                        Entry::Occupied(mut x) => x.get_mut().add_age(age),
                        Entry::Vacant(x) => {
                            x.insert(PythPriceStatsSingle::new(age));
                        }
                    }
                }
                publish_time
            }
            None => {
                tracing::error!("Could not convert Pyth publish time to NaiveDateTime, ignoring");
                now
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
