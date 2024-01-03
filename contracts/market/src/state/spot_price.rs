use std::collections::{btree_map::Entry, BTreeMap};

use crate::prelude::*;
use anyhow::ensure;
#[cfg(feature = "sei")]
use cosmwasm_std::QuerierWrapper;
use cosmwasm_std::{Binary, Order};
use msg::contracts::market::{
    entry::{
        OraclePriceFeedPythResp, OraclePriceFeedSeiResp, OraclePriceFeedSimpleResp,
        OraclePriceFeedStrideResp, PriceForQuery,
    },
    spot_price::{events::SpotPriceEvent, SpotPriceConfig, SpotPriceFeed, SpotPriceFeedData},
};
use pyth_sdk_cw::{PriceFeedResponse, PriceIdentifier};
#[cfg(feature = "sei")]
use sei_cosmwasm::{ExchangeRatesResponse, SeiQuerier};
use serde::{Deserialize, Serialize};

/// Stores spot price history.
/// Key is a [Timestamp] of when the price was received.
/// The price is only valid in the subsequent block.
const PRICES: Map<Timestamp, PriceStorage> = Map::new(namespace::PRICES);

/// Mostly for testing purposes, where we stash and later read the spot price manually
/// instead of reaching out to an oracle
const MANUAL_SPOT_PRICE: Item<PriceStorage> = Item::new(namespace::MANUAL_SPOT_PRICE);

/// The price components that are stored in [PRICES].
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct PriceStorage {
    pub(crate) price: Price,
    price_usd: PriceCollateralInUsd,
    /// Store the original incoming price in base to avoid rounding errors.
    price_base: PriceBaseInQuote,
    /// Latest price publish time for the feeds composing the price, if available
    ///
    /// Note that since deferred execution, these values will always be None.
    publish_time: Option<Timestamp>,
    /// Latest price publish time for the feeds composing the price_usd, if available
    ///
    /// Note that since deferred execution, these values will always be None.
    publish_time_usd: Option<Timestamp>,
    /// The timestamp of the block where this price point was added.
    ///
    /// Note that this only started being populated after deferred execution.
    block_time: Option<Timestamp>,
}

/// internal struct for satisfying both OraclePrice queries and spot price storage
pub(crate) struct OraclePriceInternal {
    /// A map of each pyth id used in this market to the price and publish time
    pub pyth: BTreeMap<PriceIdentifier, OraclePriceFeedPythResp>,
    /// A map of each sei denom used in this market to the price
    pub sei: BTreeMap<String, OraclePriceFeedSeiResp>,
    /// A map of each stride denom used in this market to the redemption price
    pub stride: BTreeMap<String, OraclePriceFeedStrideResp>,
    /// A map of each simple contract used in this market to the redemption price
    pub simple: BTreeMap<Addr, OraclePriceFeedSimpleResp>,
}

impl OraclePriceInternal {
    /// Calculate the publish time for this feed, ensuring we don't violate the volitile diff rule.
    ///
    /// Returns `Ok(None)` if there are no volatile feeds with a publish time.
    fn calculate_publish_time(&self, volatile_diff_seconds: u32) -> Result<Option<Timestamp>> {
        let mut oldest_newest = None::<(Timestamp, Timestamp)>;

        let mut add_new_timestamp = |timestamp| {
            oldest_newest = Some(match oldest_newest {
                None => (timestamp, timestamp),
                Some((oldest, newest)) => (oldest.min(timestamp), newest.max(timestamp)),
            });
        };

        for pyth in self.pyth.values() {
            if pyth.volatile {
                add_new_timestamp(pyth.publish_time);
            }
        }
        for sei in self.sei.values() {
            if sei.volatile {
                add_new_timestamp(sei.publish_time);
            }
        }
        for stride in self.stride.values() {
            if stride.volatile {
                add_new_timestamp(stride.publish_time);
            }
        }
        for simple in self.simple.values() {
            if simple.volatile {
                if let Some(timestamp) = simple.timestamp {
                    add_new_timestamp(timestamp);
                }
            }
        }

        match oldest_newest {
            Some((oldest, newest)) => {
                debug_assert!(oldest <= newest);
                let diff = newest.checked_sub(oldest, "calculate_publish_time")?;
                let allowed = Duration::from_seconds(volatile_diff_seconds.into());
                if diff <= allowed {
                    Ok(Some(oldest))
                } else {
                    Err(MarketError::VolatilePriceFeedTimeDelta { oldest, newest }.into_anyhow())
                }
            }
            None => Ok(None),
        }
    }

    pub fn compose_price(
        &self,
        market_id: &MarketId,
        feeds: &[SpotPriceFeed],
        feeds_usd: &[SpotPriceFeed],
        block_time: Timestamp,
    ) -> Result<PriceStorage> {
        let price_amount = self.compose_price_feeds(feeds)?;

        let price_amount_usd = self.compose_price_feeds(feeds_usd)?;

        let market_type = market_id.get_market_type();
        let price_base = PriceBaseInQuote::from_non_zero(price_amount);
        let price = price_base.into_notional_price(market_type);
        let price_usd = PriceCollateralInUsd::from_non_zero(price_amount_usd);

        Ok(PriceStorage {
            price,
            price_usd,
            price_base,
            publish_time: None,
            publish_time_usd: None,
            block_time: Some(block_time),
        })
    }

    // given a list of feeds, compose them into a single price and publish_time (if available)
    pub fn compose_price_feeds(&self, feeds: &[SpotPriceFeed]) -> Result<NumberGtZero> {
        let mut acc_price: Option<Number> = None;

        for SpotPriceFeed {
            data,
            inverted,
            volatile: _,
        } in feeds
        {
            let price = match data {
                SpotPriceFeedData::Pyth { id, .. } => self
                    .pyth
                    .get(id)
                    .map(|x| x.price)
                    .with_context(|| format!("no pyth price for id {}", id))?,
                SpotPriceFeedData::Sei { denom } => self
                    .sei
                    .get(denom)
                    .map(|x| x.price)
                    .with_context(|| format!("no sei price for denom {}", denom))?,
                SpotPriceFeedData::Stride { denom, .. } => self
                    .stride
                    .get(denom)
                    .map(|x| x.redemption_rate)
                    .with_context(|| format!("no stride redemption rate for denom {}", denom))?,
                SpotPriceFeedData::Constant { price } => *price,
                SpotPriceFeedData::Simple { contract, .. } => self
                    .simple
                    .get(contract)
                    .map(|x| x.value)
                    .with_context(|| format!("no simple price for contract {}", contract))?,
            };

            let price = if *inverted {
                Number::ONE / price.into_number()
            } else {
                price.into_number()
            };

            acc_price = match acc_price {
                None => Some(price),
                Some(prev_price) => Some(prev_price * price),
            }
        }

        match acc_price {
            Some(price) => {
                let price = NumberGtZero::try_from(price)?;
                Ok(price)
            }
            None => anyhow::bail!("No price feeds provided"),
        }
    }
}

impl State<'_> {
    pub(crate) fn make_price_point(
        &self,
        store: &dyn Storage,
        timestamp: Timestamp,
        PriceStorage {
            price,
            price_usd,
            price_base,
            publish_time,
            publish_time_usd,
            block_time: _,
        }: PriceStorage,
    ) -> Result<PricePoint> {
        let market_id = self.market_id(store)?;
        let market_type = market_id.get_market_type();

        Ok(PricePoint {
            timestamp,
            price_notional: price,
            price_usd,
            price_base,
            is_notional_usd: market_id.is_notional_usd(),
            market_type,
            publish_time,
            publish_time_usd,
        })
    }

    /// Returns the spot price for the provided timestamp.
    /// If no timestamp is provided, it returns the latest spot price.
    pub(crate) fn spot_price(
        &self,
        store: &dyn Storage,
        timestamp: Timestamp,
    ) -> Result<PricePoint> {
        self.spot_price_inner_opt(store, timestamp)?.ok_or_else(|| {
            perp_error!(
                ErrorId::PriceNotFound,
                ErrorDomain::SpotPrice,
                "there is no spot price for timestamp {}",
                timestamp
            )
            .into()
        })
    }

    fn spot_price_inner_opt(
        &self,
        store: &dyn Storage,
        timestamp: Timestamp,
    ) -> Result<Option<PricePoint>> {
        let max = Bound::inclusive(timestamp);

        match PRICES
            .range(store, None, Some(max), Order::Descending)
            .next()
        {
            None => Ok(None),
            Some(Err(e)) => Err(e.into()),
            Some(Ok((timestamp, price_storage))) => self
                .make_price_point(store, timestamp, price_storage)
                .map(Some),
        }
    }

    /// For queries that provide a fresh price from the outside
    /// override the current price with the given value.
    ///
    /// This doesn't store any data in the contract. Instead, it only updates an
    /// in-memory representation.
    ///
    /// Since users are providing the price, the `publish_time` is always None
    /// and the `timestamp` is always now
    pub(crate) fn override_current_price(
        &self,
        store: &dyn Storage,
        price: Option<PriceForQuery>,
    ) -> Result<()> {
        if let Some(PriceForQuery { base, collateral }) = price {
            let market_id = self.market_id(store)?;
            let market_type = market_id.get_market_type();
            let price = base.into_notional_price(market_type);

            let price = PricePoint {
                price_notional: price,
                price_usd: collateral,
                price_base: base,
                // Only used for simulating new prices not in the contract, so self.now() is OK
                timestamp: self.now(),
                is_notional_usd: market_id.is_notional_usd(),
                market_type,
                publish_time: None,
                publish_time_usd: None,
            };

            self.spot_price_cache
                .set(price)
                .map_err(|_| anyhow!("override_current_price: current price already loaded"))?;
        }
        Ok(())
    }

    /// Get the current spot price
    pub(crate) fn current_spot_price(&self, store: &dyn Storage) -> Result<PricePoint> {
        self.spot_price_cache
            .get_or_try_init(|| self.spot_price(store, self.now()))
            .copied()
    }

    pub(crate) fn historical_spot_prices(
        &self,
        store: &dyn Storage,
        start_after: Option<Timestamp>,
        limit: Option<usize>,
        order: Option<Order>,
    ) -> Result<Vec<PricePoint>> {
        let order = order.unwrap_or(Order::Descending);
        let iter = PRICES
            .range(
                store,
                match order {
                    Order::Ascending => start_after.map(Bound::exclusive),
                    Order::Descending => None,
                },
                match order {
                    Order::Ascending => None,
                    Order::Descending => start_after.map(Bound::exclusive),
                },
                order,
            )
            .map(|res| {
                let (timestamp, price_storage) = res?;
                self.make_price_point(store, timestamp, price_storage)
            });

        let prices = match limit {
            None => iter.collect::<Result<Vec<_>>>()?,
            Some(limit) => iter.take(limit).collect::<Result<Vec<_>>>()?,
        };

        Ok(prices)
    }

    /// Get the next price point after the given minimum bound.
    pub(crate) fn spot_price_after(
        &self,
        store: &dyn Storage,
        min: Option<Bound<Timestamp>>,
    ) -> Result<Option<PricePoint>> {
        match PRICES
            .range(store, min, None, Order::Ascending)
            .next()
            .transpose()?
        {
            Some((timestamp, price_storage)) => self
                .make_price_point(store, timestamp, price_storage)
                .map(Some),
            None => Ok(None),
        }
    }

    pub(crate) fn save_manual_spot_price(
        &self,
        ctx: &mut StateContext,
        price_base: PriceBaseInQuote,
        price_usd: PriceCollateralInUsd,
    ) -> Result<()> {
        let market_id = self.market_id(ctx.storage)?;
        let market_type = market_id.get_market_type();
        let price = price_base.into_notional_price(market_type);

        MANUAL_SPOT_PRICE
            .save(
                ctx.storage,
                &PriceStorage {
                    price,
                    price_usd,
                    price_base,
                    publish_time: None,
                    publish_time_usd: None,
                    // Acceptable to use self.now() for manual price updates, no real publish time
                    block_time: Some(self.now()),
                },
            )
            .map_err(|err| err.into())
    }

    pub(crate) fn spot_price_append(&self, ctx: &mut StateContext) -> Result<()> {
        let market_id = self.market_id(ctx.storage)?;

        let (new_publish_time, price_storage) = match self.config.spot_price.clone() {
            SpotPriceConfig::Manual { .. } => (
                self.now(),
                MANUAL_SPOT_PRICE.may_load(ctx.storage)?.context(
                    "This contract has manual price updates, and no price has been set yet.",
                )?,
            ),
            SpotPriceConfig::Oracle {
                pyth: _,
                stride: _,
                feeds,
                feeds_usd,
                volatile_diff_seconds,
            } => {
                let internal = self.get_oracle_price(true)?;
                const DEFAULT_VOLATILE_DIFF_SECONDS: u32 = 5;
                let new_publish_time = internal
                    .calculate_publish_time(
                        volatile_diff_seconds.unwrap_or(DEFAULT_VOLATILE_DIFF_SECONDS),
                    )?
                    .ok_or(MarketError::NoPricePublishTimeFound.into_anyhow())?;
                // self.now() usage is OK, it's explicitly for saving the block time in storage
                let price_storage =
                    internal.compose_price(market_id, &feeds, &feeds_usd, self.now())?;
                (new_publish_time, price_storage)
            }
        };

        // Ensure strictly monotonically increasing publish price timestamps. Find the most recently published timestamp and make sure our new publish time is greater than it.
        match PRICES
            .keys(ctx.storage, None, None, Order::Descending)
            .next()
        {
            // No prices published yet, so we're allowed to use this timestamp
            None => (),
            Some(last_published) => {
                if last_published? >= new_publish_time {
                    // New publish time is not newer that the last published time, do not add a new spot price
                    return Ok(());
                }
            }
        }

        // sanity check
        if let Some(price_usd) = price_storage.price_base.try_into_usd(market_id) {
            ensure!(
                price_storage.price_usd == price_usd,
                "Price in USD mismatch {} != {}",
                price_storage.price_usd,
                price_usd
            );
        }

        ctx.response_mut().add_event(SpotPriceEvent {
            timestamp: new_publish_time,
            price_usd: price_storage.price_usd,
            price_notional: price_storage.price,
            price_base: price_storage.price_base,
            publish_time: price_storage.publish_time,
            publish_time_usd: price_storage.publish_time_usd,
        });

        PRICES
            .save(ctx.storage, new_publish_time, &price_storage)
            .map_err(|err| err.into())
    }

    pub(crate) fn get_oracle_price(&self, validate_age: bool) -> Result<OraclePriceInternal> {
        match self.config.spot_price.clone() {
            SpotPriceConfig::Manual { .. } => {
                bail!("Manual spot price does not have an oracle price");
            }
            SpotPriceConfig::Oracle {
                pyth: pyth_config,
                stride: stride_config,
                feeds,
                feeds_usd,
                volatile_diff_seconds: _,
            } => {
                let mut pyth = BTreeMap::new();
                let mut stride = BTreeMap::new();
                let mut simple = BTreeMap::new();
                #[cfg(feature = "sei")]
                let mut sei = BTreeMap::new();
                #[cfg(not(feature = "sei"))]
                let sei = BTreeMap::new();

                let current_block_time_seconds = self.env.block.time.seconds().try_into()?;

                for feed in feeds.iter().chain(feeds_usd.iter()) {
                    match &feed.data {
                        SpotPriceFeedData::Pyth {
                            id,
                            age_tolerance_seconds,
                        } => {
                            if let Entry::Vacant(entry) = pyth.entry(*id) {
                                let pyth_config = pyth_config
                                    .as_ref()
                                    .context("pyth feeds need a pyth config!")?;

                                let price_feed_response: PriceFeedResponse =
                                    pyth_sdk_cw::query_price_feed(
                                        &self.querier,
                                        pyth_config.contract_address.clone(),
                                        *id,
                                    )?;

                                let price_feed = price_feed_response.price_feed;

                                let price = if validate_age {
                                    price_feed
                                        // alternative: .get_emaprice_no_older_than()
                                        .get_price_no_older_than(
                                            current_block_time_seconds,
                                            (*age_tolerance_seconds).into(),
                                        )
                                        .ok_or_else(|| {
                                            perp_error!(
                                                ErrorId::PriceTooOld,
                                                ErrorDomain::Pyth,
                                                "Current price is not available. Price id: {}, Current block time: {}, price publish time: {}, diff: {}, age_tolerance: {}",
                                                id,
                                                current_block_time_seconds,
                                                price_feed.get_price_unchecked().publish_time,
                                                (price_feed.get_price_unchecked().publish_time - current_block_time_seconds).abs(),
                                                age_tolerance_seconds
                                            )
                                        })?
                                } else {
                                    price_feed.get_price_unchecked()
                                };

                                let publish_time =
                                    Timestamp::from_seconds(price.publish_time.try_into()?);
                                let price = Number::try_from(price)?;
                                let price =
                                    NumberGtZero::try_from(price).context("price must be > 0")?;

                                entry.insert(OraclePriceFeedPythResp {
                                    price,
                                    publish_time,
                                    // Pyth feeds default to being volatile unless otherwise overridden
                                    volatile: feed.volatile.unwrap_or(true),
                                });
                            }
                        }

                        SpotPriceFeedData::Sei { denom } => {
                            #[cfg(feature = "sei")]
                            {
                                if let Entry::Vacant(entry) = sei.entry(denom.clone()) {
                                    let querier = QuerierWrapper::new(&*self.querier);
                                    let querier = SeiQuerier::new(&querier);
                                    let res: ExchangeRatesResponse =
                                        querier.query_exchange_rates()?;
                                    let pair = res
                                        .denom_oracle_exchange_rate_pairs
                                        .iter()
                                        .find(|x| x.denom == *denom)
                                        .with_context(|| format!("no such denom {denom}"))?;

                                    let price: Decimal256 =
                                        pair.oracle_exchange_rate.exchange_rate.into();
                                    let price = Number::try_from(price)?;
                                    let price = NumberGtZero::try_from(price)
                                        .context("price must be > 0")?;

                                    let publish_time = Timestamp::from_millis(
                                        pair.oracle_exchange_rate.last_update_timestamp,
                                    );

                                    entry.insert(OraclePriceFeedSeiResp {
                                        price,
                                        publish_time,
                                        // Sei feeds default to being volatile unless otherwise overridden
                                        volatile: feed.volatile.unwrap_or(true),
                                    });
                                }
                            }
                            #[cfg(not(feature = "sei"))]
                            {
                                bail!("SEI price feed for {denom} is only available on sei network")
                            }
                        }

                        SpotPriceFeedData::Stride {
                            denom,
                            age_tolerance_seconds,
                        } => {
                            if let Entry::Vacant(entry) = stride.entry(denom.clone()) {
                                let stride_address = &stride_config
                                    .as_ref()
                                    .context("stride config not set")?
                                    .contract_address;

                                #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
                                #[serde(rename_all = "snake_case")]
                                enum StrideQuery {
                                    RedemptionRate {
                                        /// The denom should be the ibc hash of an stToken as it lives on the oracle chain
                                        /// (e.g. ibc/{hash(transfer/channel-326/stuatom)} on Osmosis)
                                        denom: String,
                                        /// Params should always be None
                                        params: Option<Binary>,
                                    },
                                }

                                #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
                                #[serde(rename_all = "snake_case")]
                                pub struct RedemptionRateResponse {
                                    pub redemption_rate: Decimal256,
                                    pub update_time: u64,
                                }

                                let resp: RedemptionRateResponse = self.querier.query_wasm_smart(
                                    stride_address,
                                    &StrideQuery::RedemptionRate {
                                        denom: denom.to_string(),
                                        params: None,
                                    },
                                )?;

                                if validate_age {
                                    if let Some(time_diff) =
                                        u64::try_from(current_block_time_seconds)?
                                            .checked_sub(resp.update_time)
                                    {
                                        if time_diff > (*age_tolerance_seconds).into() {
                                            perp_bail!(
                                                ErrorId::PriceTooOld,
                                                ErrorDomain::Stride,
                                                "Current price is not available. Price denom: {}, Current block time: {}, price publish time: {}, diff: {}, age_tolerance: {}",
                                                denom,
                                                current_block_time_seconds,
                                                resp.update_time,
                                                time_diff,
                                                age_tolerance_seconds
                                            )
                                        }
                                    }
                                }

                                let publish_time = Timestamp::from_seconds(resp.update_time);
                                let redemption_rate = Number::try_from(resp.redemption_rate)?;
                                let redemption_rate = NumberGtZero::try_from(redemption_rate)
                                    .context("redemption_rate must be > 0")?;

                                entry.insert(OraclePriceFeedStrideResp {
                                    redemption_rate,
                                    publish_time,
                                    // Stride feeds default to being non-volatile unless otherwise overridden
                                    volatile: feed.volatile.unwrap_or_default(),
                                });
                            }
                        }

                        SpotPriceFeedData::Constant { .. } => {
                            // nothing to do here, constant prices are used without a lookup
                        }
                        SpotPriceFeedData::Simple {
                            contract,
                            age_tolerance_seconds,
                        } => {
                            if let Entry::Vacant(entry) = simple.entry(contract.clone()) {
                                #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
                                #[serde(rename_all = "snake_case")]
                                enum SimpleQuery {
                                    Price {},
                                }

                                let resp: OraclePriceFeedSimpleResp = self
                                    .querier
                                    .query_wasm_smart(contract, &SimpleQuery::Price {})?;

                                if validate_age {
                                    let publish_time =
                                        resp.timestamp.unwrap_or(resp.block_info.time.into());
                                    let time_diff = self
                                        .now()
                                        .checked_sub(publish_time, "simple oracle price time")?;
                                    if time_diff
                                        > Duration::from_seconds((*age_tolerance_seconds).into())
                                    {
                                        perp_bail!(
                                            ErrorId::PriceTooOld,
                                            ErrorDomain::SimpleOracle,
                                            "Current price is not available on simple oracle. Price contract: {}, Current block time: {}, price publish time: {}, diff: {:?}, age_tolerance: {}",
                                            contract,
                                            current_block_time_seconds,
                                            publish_time,
                                            time_diff,
                                            age_tolerance_seconds
                                        )
                                    }
                                }

                                entry.insert(resp);
                            }
                        }
                    }
                }

                Ok(OraclePriceInternal {
                    pyth,
                    stride,
                    sei,
                    simple,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_inverted_compose() {
        let eth_usd = pyth_sdk_cw::Price {
            price: 179276800001,
            conf: 0,
            expo: -8,
            publish_time: 0,
        };

        let btc_usd = pyth_sdk_cw::Price {
            price: 2856631500000,
            conf: 0,
            expo: -8,
            publish_time: 0,
        };

        let eth = Number::try_from(eth_usd).unwrap();
        let btc = Number::try_from(btc_usd).unwrap();
        let btc = Number::ONE / btc;
        let eth_btc = eth * btc;

        assert_eq!(eth_btc, Number::try_from("0.062758112133468261").unwrap());
    }
}
