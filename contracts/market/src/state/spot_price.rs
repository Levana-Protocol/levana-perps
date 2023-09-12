use crate::prelude::*;
use cosmwasm_std::Order;
use msg::contracts::market::{
    entry::PriceForQuery,
    spot_price::{
        events::SpotPriceEvent, PythConfig, PythPriceServiceNetwork, SpotPriceConfig,
        SpotPriceFeed, SpotPriceFeedData,
    },
};
use pyth_sdk_cw::PriceFeedResponse;

/// Stores spot price history.
/// Key is a [Timestamp] of when the price was received.
/// The price is only valid in the subsequent block.
const PRICES: Map<Timestamp, PriceStorage> = Map::new(namespace::PRICES);

/// Mostly for testing purposes, where we stash and later read the spot price manually
/// instead of reaching out to an oracle
const MANUAL_SPOT_PRICE: Item<PriceStorage> = Item::new(namespace::MANUAL_SPOT_PRICE);

/// The price components that are stored in [PRICES].
#[derive(serde::Serialize, serde::Deserialize)]
struct PriceStorage {
    price: Price,
    price_usd: PriceCollateralInUsd,
    /// Store the original incoming price in base to avoid rounding errors.
    price_base: PriceBaseInQuote,
    /// Latest price publish time for the feeds composing the price, if available
    publish_time: Option<Timestamp>,
    /// Latest price publish time for the feeds composing the price_usd, if available
    publish_time_usd: Option<Timestamp>,
}

impl State<'_> {
    fn make_price_point(
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

    /// Override the current price with the given value.
    ///
    /// This doesn't store any data in the contract. Instead, it only updates an
    /// in-memory representation.
    pub(crate) fn override_current_price(
        &self,
        store: &dyn Storage,
        price: Option<PriceForQuery>,
    ) -> Result<()> {
        if let Some(PriceForQuery { base, collateral }) = price {
            let market_id = self.market_id(store)?;
            let market_type = market_id.get_market_type();
            let price_usd = get_price_usd(base, collateral, market_id)?;
            let price = base.into_notional_price(market_type);

            let (publish_time, publish_time_usd) = self
                .spot_price_cache
                .get()
                .map_or((None, None), |x| (x.publish_time, x.publish_time_usd));

            let price = PricePoint {
                price_notional: price,
                price_usd,
                price_base: base,
                timestamp: self.now(),
                is_notional_usd: market_id.is_notional_usd(),
                market_type,
                publish_time,
                publish_time_usd,
            };

            self.spot_price_cache
                .set(price)
                .ok()
                .context("override_current_price: current price already loaded")?;
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
        price_usd: Option<PriceCollateralInUsd>,
    ) -> Result<()> {
        let market_id = self.market_id(ctx.storage)?;
        let market_type = market_id.get_market_type();
        let price = price_base.into_notional_price(market_type);
        let price_usd = get_price_usd(price_base, price_usd, market_id)?;

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
            return Ok(());
            // DISCUSS: This used to be an error... no more?
            // perp_bail!(
            //     ErrorId::PriceAlreadyExists,
            //     ErrorDomain::SpotPrice,
            //     "price already exist for timestamp {}",
            //     timestamp
            // );
        }

        let price_storage = match &self.config.spot_price {
            SpotPriceConfig::Manual => MANUAL_SPOT_PRICE.load(ctx.storage)?,
            SpotPriceConfig::Oracle {
                pyth,
                feeds,
                feeds_usd,
            } => {
                let (price_amount, publish_time) = self.get_oracle_price(pyth.as_ref(), feeds)?;

                let (price_usd, publish_time_usd) = match feeds_usd {
                    None => (None, None),
                    Some(feeds_usd) => {
                        let (price_amount_usd, publish_time_usd) =
                            self.get_oracle_price(pyth.as_ref(), feeds_usd)?;
                        (
                            Some(PriceCollateralInUsd::from_non_zero(price_amount_usd)),
                            publish_time_usd,
                        )
                    }
                };

                let market_id = self.market_id(ctx.storage)?;
                let market_type = market_id.get_market_type();
                let price_base = PriceBaseInQuote::from_non_zero(price_amount);
                let price = price_base.into_notional_price(market_type);
                let price_usd = get_price_usd(price_base, price_usd, market_id)?;

                PriceStorage {
                    price,
                    price_usd,
                    price_base,
                    publish_time,
                    publish_time_usd,
                }
            }
        };

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

    pub(crate) fn get_oracle_price(
        &self,
        pyth: Option<&PythConfig>,
        feeds: &[SpotPriceFeed],
    ) -> Result<(NumberGtZero, Option<Timestamp>)> {
        let mut acc_price: Option<(Number, Option<Timestamp>)> = None;

        for SpotPriceFeed { data, inverted } in feeds {
            let (price, publish_time) = match data {
                SpotPriceFeedData::Pyth { id, network } => {
                    let pyth = pyth.context("pyth feeds need a pyth config!")?;

                    let price_feed_response: PriceFeedResponse = match network {
                        PythPriceServiceNetwork::Stable => pyth_sdk_cw::query_price_feed(
                            &self.querier,
                            pyth.oracle_address_stable.clone(),
                            *id,
                        )?,
                        PythPriceServiceNetwork::Edge => pyth_sdk_cw::query_price_feed(
                            &self.querier,
                            pyth.oracle_address_edge.clone(),
                            *id,
                        )?,
                    };

                    let price_feed = price_feed_response.price_feed;

                    let current_block_time_seconds = self.env.block.time.seconds().try_into()?;
                    let price = price_feed
                        // alternative: .get_emaprice_no_older_than()
                        .get_price_no_older_than(
                            current_block_time_seconds,
                            pyth.age_tolerance_seconds,
                        )
                        .ok_or_else(|| {
                            perp_error!(
                                ErrorId::PriceTooOld,
                                ErrorDomain::Pyth,
                                "Current price is not available. Price id: {}, inverted: {}, Current block time: {}, price publish time: {}, diff: {}, age_tolerance: {}",
                                id,
                                inverted,
                                current_block_time_seconds,
                                price_feed.get_price_unchecked().publish_time,
                                (price_feed.get_price_unchecked().publish_time - current_block_time_seconds).abs(),
                                pyth.age_tolerance_seconds
                            )
                        })?;

                    let publish_time =
                        Some(Timestamp::from_seconds(price.publish_time.try_into()?));
                    let price: Number = Number::try_from(price)?;

                    (price, publish_time)
                }
                SpotPriceFeedData::Sei { .. } => {
                    // TODO: query the native module and get the price, no publish time
                    todo!("Implement SEI price feed")
                }
                SpotPriceFeedData::Stride { .. } => {
                    // TODO: query the contract and get the redemption price etc., no publish time
                    todo!("Implement Stride price feed")
                }
            };

            acc_price = match acc_price {
                None => Some((price, publish_time)),
                Some((prev_price, prev_publish_time)) => {
                    let publish_time = publish_time.max(prev_publish_time);
                    let next_price =
                        compose_price(prev_price.into_number(), price.into_number(), *inverted)?;
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

fn compose_price(prev: Number, mut curr: Number, curr_inverted: bool) -> Result<Number> {
    if curr_inverted {
        curr = Number::ONE / curr;
    }

    Ok(prev * curr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyth_route_compose() {
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

        let eth_btc = compose_price(
            eth_usd.try_into().unwrap(),
            btc_usd.try_into().unwrap(),
            true,
        )
        .unwrap();

        assert_eq!(eth_btc, Number::try_from("0.062758112133468261").unwrap());
    }
}

fn get_price_usd(
    price: PriceBaseInQuote,
    price_usd: Option<PriceCollateralInUsd>,
    market_id: &MarketId,
) -> Result<PriceCollateralInUsd> {
    match (derive_price_usd(price, market_id), price_usd) {
        (None, None) => anyhow::bail!("Must provide a price in USD"),
        (None, Some(price_usd)) => Ok(price_usd),
        (Some(price_usd), None) => Ok(price_usd),
        (Some(x), Some(y)) => {
            if x == y {
                Ok(y)
            } else {
                Err(anyhow::anyhow!("Provided conflicting price information in USD. Price base in quote: {price}. Price collateral in USD: {y}. Derived price: {x}"))
            }
        }
    }
}

fn derive_price_usd(price: PriceBaseInQuote, market_id: &MarketId) -> Option<PriceCollateralInUsd> {
    // For comments below, assume we're dealing with a pair between USD and ATOM
    if market_id.get_base() == "USD" {
        Some(match market_id.get_market_type() {
            MarketType::CollateralIsQuote => {
                // Base == USD, quote == collateral == ATOM
                // price = ATOM/USD
                // Return value = USD/ATOM
                //
                // Therefore, we need to invert the numbers
                PriceCollateralInUsd::from_non_zero(price.into_non_zero().inverse())
            }
            MarketType::CollateralIsBase => {
                // Base == collateral == USD
                // Return value == USD/USD
                // QED it's one
                PriceCollateralInUsd::one()
            }
        })
    } else if market_id.get_quote() == "USD" {
        Some(match market_id.get_market_type() {
            MarketType::CollateralIsQuote => {
                // Collateral == quote == USD
                // Return value = USD/USD
                // QED it's one
                PriceCollateralInUsd::one()
            }
            MarketType::CollateralIsBase => {
                // Collateral == base == ATOM
                // Quote == USD
                // Price = USD/ATOM
                // Return value = USD/ATOM
                // QED same number
                PriceCollateralInUsd::from_non_zero(price.into_non_zero())
            }
        })
    } else {
        // Neither asset is USD, so we can't get a price
        None
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
        // These do not require a spot_price_append
        ExecuteMsg::CancelLimitOrder { .. } => false,
        ExecuteMsg::ClaimYield { .. } => false,
        ExecuteMsg::CollectUnstakedLp { .. } => false,
        ExecuteMsg::LiquidityTokenProxy { .. } => false,
        ExecuteMsg::NftProxy { .. } => false,
        ExecuteMsg::Owner { .. } => false,
        ExecuteMsg::PlaceLimitOrder { .. } => false,
        ExecuteMsg::ProvideCrankFunds { .. } => false,
        ExecuteMsg::Receive { .. } => false,
        ExecuteMsg::SetTriggerOrder { .. } => false,
        ExecuteMsg::StakeLp { .. } => false,
        ExecuteMsg::StopUnstakingXlp { .. } => false,
        ExecuteMsg::TransferDaoFees { .. } => false,
        ExecuteMsg::UnstakeXlp { .. } => false,
    }
}
