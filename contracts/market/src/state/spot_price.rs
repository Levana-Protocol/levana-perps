use std::collections::{btree_map::Entry, BTreeMap};

use crate::prelude::*;
use anyhow::ensure;
#[cfg(feature = "sei")]
use cosmwasm_std::QuerierWrapper;
use cosmwasm_std::{Binary, Order};
use msg::contracts::market::{
    entry::{OraclePriceFeedPythResp, OraclePriceFeedStrideResp, PriceForQuery},
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
    price: Price,
    price_usd: PriceCollateralInUsd,
    /// Store the original incoming price in base to avoid rounding errors.
    price_base: PriceBaseInQuote,
    /// Latest price publish time for the feeds composing the price, if available
    publish_time: Option<Timestamp>,
    /// Latest price publish time for the feeds composing the price_usd, if available
    publish_time_usd: Option<Timestamp>,
}

/// internal struct for satisfying both OraclePrice queries and spot price storage
pub(crate) struct OraclePriceInternal {
    /// A map of each pyth id used in this market to the price and publish time
    pub pyth: BTreeMap<PriceIdentifier, OraclePriceFeedPythResp>,
    /// A map of each sei denom used in this market to the price
    pub sei: BTreeMap<String, NumberGtZero>,
    /// A map of each stride denom used in this market to the redemption price
    pub stride: BTreeMap<String, OraclePriceFeedStrideResp>,
}

impl OraclePriceInternal {
    pub fn compose_price(
        &self,
        market_id: &MarketId,
        feeds: &[SpotPriceFeed],
        feeds_usd: &[SpotPriceFeed],
    ) -> Result<PriceStorage> {
        let (price_amount, publish_time) = self.compose_price_feeds(feeds)?;

        let (price_amount_usd, publish_time_usd) = self.compose_price_feeds(feeds_usd)?;

        let market_type = market_id.get_market_type();
        let price_base = PriceBaseInQuote::from_non_zero(price_amount);
        let price = price_base.into_notional_price(market_type);
        let price_usd = PriceCollateralInUsd::from_non_zero(price_amount_usd);

        Ok(PriceStorage {
            price,
            price_usd,
            price_base,
            publish_time,
            publish_time_usd,
        })
    }

    // given a list of feeds, compose them into a single price and publish_time (if available)
    pub fn compose_price_feeds(
        &self,
        feeds: &[SpotPriceFeed],
    ) -> Result<(NumberGtZero, Option<Timestamp>)> {
        let mut acc_price: Option<(Number, Option<Timestamp>)> = None;

        for SpotPriceFeed { data, inverted } in feeds {
            let (price, publish_time) = match data {
                SpotPriceFeedData::Pyth { id } => self
                    .pyth
                    .get(id)
                    .map(|x| (x.price, Some(x.publish_time)))
                    .with_context(|| format!("no pyth price for id {}", id))?,
                SpotPriceFeedData::Sei { denom } => self
                    .sei
                    .get(denom)
                    .map(|x| (*x, None))
                    .with_context(|| format!("no sei price for denom {}", denom))?,
                SpotPriceFeedData::Stride { denom } => self
                    .stride
                    .get(denom)
                    .map(|x| (x.redemption_rate, Some(x.publish_time)))
                    .with_context(|| format!("no stride redemption rate for denom {}", denom))?,
                SpotPriceFeedData::Constant { price } => (*price, None),
            };

            let price = if *inverted {
                Number::ONE / price.into_number()
            } else {
                price.into_number()
            };

            acc_price = match acc_price {
                None => Some((price, publish_time)),
                Some((prev_price, prev_publish_time)) => {
                    let publish_time = publish_time.max(prev_publish_time);
                    let next_price = prev_price * price;
                    Some((next_price, publish_time))
                }
            }
        }

        match acc_price {
            Some((price, publish_time)) => {
                let price = NumberGtZero::try_from(price)?;
                Ok((price, publish_time))
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
    fn spot_price_inner(&self, store: &dyn Storage, timestamp: Timestamp) -> Result<PricePoint> {
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

    /// Returns the spot price for the provided timestamp.
    /// If no timestamp is provided, it returns the latest spot price.
    pub(crate) fn spot_price(
        &self,
        store: &dyn Storage,
        time: Option<Timestamp>,
    ) -> Result<PricePoint> {
        match time {
            None => self
                .spot_price_cache
                .get_or_try_init(|| self.spot_price_inner(store, self.now()))
                .copied(),
            Some(time) => self.spot_price_inner(store, time),
        }
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

    /// Get the latest spot price, if one is set.
    pub(crate) fn spot_price_latest_opt(&self, store: &dyn Storage) -> Result<Option<PricePoint>> {
        if let Some(x) = self.spot_price_cache.get() {
            return Ok(Some(*x));
        }

        match self.spot_price_inner_opt(store, self.now()) {
            Ok(Some(x)) => {
                self.spot_price_cache.set(x).ok();
                Ok(Some(x))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
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
                },
            )
            .map_err(|err| err.into())
    }

    pub(crate) fn spot_price_append(&self, ctx: &mut StateContext) -> Result<()> {
        let timestamp = self.now();

        if PRICES.has(ctx.storage, timestamp) {
            // if price changes within the same block, we don't care - first come first serve
            return Ok(());
        }

        let market_id = self.market_id(ctx.storage)?;

        let price_storage = match self.config.spot_price.clone() {
            SpotPriceConfig::Manual { .. } => MANUAL_SPOT_PRICE.load(ctx.storage)?,
            SpotPriceConfig::Oracle {
                pyth: _,
                stride: _,
                feeds,
                feeds_usd,
            } => self
                .get_oracle_price(true)?
                .compose_price(market_id, &feeds, &feeds_usd)?,
        };

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
            timestamp,
            price_usd: price_storage.price_usd,
            price_notional: price_storage.price,
            price_base: price_storage.price_base,
            publish_time: price_storage.publish_time,
            publish_time_usd: price_storage.publish_time_usd,
        });

        PRICES
            .save(ctx.storage, timestamp, &price_storage)
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
            } => {
                let mut pyth = BTreeMap::new();
                let mut stride = BTreeMap::new();
                #[cfg(feature = "sei")]
                let mut sei = BTreeMap::new();
                #[cfg(not(feature = "sei"))]
                let sei = BTreeMap::new();

                for feed in feeds.iter().chain(feeds_usd.iter()) {
                    match &feed.data {
                        SpotPriceFeedData::Pyth { id } => {
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
                                    let current_block_time_seconds =
                                        self.env.block.time.seconds().try_into()?;

                                    price_feed
                                        // alternative: .get_emaprice_no_older_than()
                                        .get_price_no_older_than(
                                            current_block_time_seconds,
                                            pyth_config.age_tolerance_seconds,
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
                                                pyth_config.age_tolerance_seconds
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

                                    entry.insert(price);
                                }
                            }
                            #[cfg(not(feature = "sei"))]
                            {
                                bail!("SEI price feed for {denom} is only available on sei network")
                            }
                        }

                        SpotPriceFeedData::Stride { denom } => {
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

                                let publish_time = Timestamp::from_seconds(resp.update_time);
                                let redemption_rate = Number::try_from(resp.redemption_rate)?;
                                let redemption_rate = NumberGtZero::try_from(redemption_rate)
                                    .context("redemption_rate must be > 0")?;

                                entry.insert(OraclePriceFeedStrideResp {
                                    redemption_rate,
                                    publish_time,
                                });
                            }
                        }

                        SpotPriceFeedData::Constant { .. } => {
                            // nothing to do here, constant prices are used without a lookup
                        }
                    }
                }

                Ok(OraclePriceInternal { pyth, stride, sei })
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

pub fn requires_spot_price_append(msg: &ExecuteMsg) -> bool {
    // explicitly listed to avoid accidentally handling new messages
    // Receive is checked via the deserialized inner message
    match msg {
        // these require a spot_price_append
        ExecuteMsg::OpenPosition { .. } => true,
        ExecuteMsg::ClosePosition { .. } => true,
        ExecuteMsg::CloseAllPositions { .. } => true,
        ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { .. } => true,
        ExecuteMsg::UpdatePositionAddCollateralImpactSize { .. } => true,
        ExecuteMsg::UpdatePositionLeverage { .. } => true,
        ExecuteMsg::UpdatePositionMaxGains { .. } => true,
        ExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage { .. } => true,
        ExecuteMsg::UpdatePositionRemoveCollateralImpactSize { .. } => true,
        ExecuteMsg::DepositLiquidity { .. } => true,
        ExecuteMsg::WithdrawLiquidity { .. } => true,
        ExecuteMsg::ReinvestYield { .. } => true,
        ExecuteMsg::Crank { .. } => true,
        ExecuteMsg::PlaceLimitOrder { .. } => true,
        ExecuteMsg::SetTriggerOrder { .. } => true,
        // These do not require a spot_price_append
        ExecuteMsg::CancelLimitOrder { .. } => false,
        ExecuteMsg::ClaimYield { .. } => false,
        ExecuteMsg::CollectUnstakedLp { .. } => false,
        ExecuteMsg::LiquidityTokenProxy { .. } => false,
        ExecuteMsg::NftProxy { .. } => false,
        ExecuteMsg::Owner { .. } => false,
        ExecuteMsg::ProvideCrankFunds { .. } => false,
        ExecuteMsg::Receive { .. } => false,
        ExecuteMsg::StakeLp { .. } => false,
        ExecuteMsg::StopUnstakingXlp { .. } => false,
        ExecuteMsg::TransferDaoFees { .. } => false,
        ExecuteMsg::UnstakeXlp { .. } => false,
        // Does do a spot_price_append, but it's handled internally after the price is set
        ExecuteMsg::SetManualPrice { .. } => false,
    }
}
