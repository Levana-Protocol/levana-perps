use crate::prelude::*;
use cosmwasm_std::Order;
use msg::contracts::market::entry::PriceForQuery;

/// Stores spot price history.
/// Key is a [Timestamp] of when the price was received.
/// The price is only valid in the subsequent block.
const PRICES: Map<Timestamp, PriceStorage> = Map::new(namespace::PRICES);

/// The price components that are stored in [PRICES].
#[derive(serde::Serialize, serde::Deserialize)]
struct PriceStorage {
    price: Price,
    price_usd: PriceCollateralInUsd,
    /// Store the original incoming price in base to avoid rounding errors.
    price_base: PriceBaseInQuote,
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
            let price = PricePoint {
                price_notional: price,
                price_usd,
                price_base: base,
                timestamp: self.now(),
                is_notional_usd: market_id.is_notional_usd(),
                market_type,
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

    pub(crate) fn spot_price_append(
        &self,
        _ctx: &mut StateContext,
    ) -> Result<()> {
        // don't forget to emit the spot price event!
        todo!()
        // let market_id = self.market_id(ctx.storage)?;
        // let timestamp = self.now();

        // if PRICES.has(ctx.storage, timestamp) {
        //     Err(perp_anyhow!(
        //         ErrorId::PriceAlreadyExists,
        //         ErrorDomain::SpotPrice,
        //         "price already exist for timestamp {}",
        //         timestamp
        //     ))
        // } else {
        //     let market_type = self.market_id(ctx.storage)?.get_market_type();
        //     let price_usd = get_price_usd(price_base, price_usd, market_id)?;
        //     let price = price_base.into_notional_price(market_type);
        //     ctx.response_mut().add_event(SpotPriceEvent {
        //         timestamp,
        //         price_usd,
        //         price_notional: price,
        //         price_base: price.into_base_price(market_type),
        //         // FIXME!
        //         publish_time: None,
        //         publish_time_usd: None,
        //     });
        //     PRICES
        //         .save(
        //             ctx.storage,
        //             timestamp,
        //             &PriceStorage {
        //                 price,
        //                 price_usd,
        //                 price_base,
        //             },
        //         )
        //         .map_err(|err| err.into())
        // }
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