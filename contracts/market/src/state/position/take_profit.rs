use std::ops::{Mul, Sub};

use crate::prelude::*;
use shared::{
    compat::{calc_notional_size, TakeProfitFromCounterCollateral},
    storage::PricePoint,
};

pub(crate) struct TakeProfitToCounterCollateral<'a> {
    pub take_profit_price_base: TakeProfitPrice,
    pub market_type: MarketType,
    pub collateral: NonZero<Collateral>,
    pub leverage_to_base: LeverageToBase,
    pub direction: DirectionToBase,
    pub config: &'a Config,
    pub price_point: &'a PricePoint,
}

impl<'a> TakeProfitToCounterCollateral<'a> {
    pub(crate) fn calc(&self) -> Result<NonZero<Collateral>> {
        let take_profit_price = self.capped_take_profit_price()?;

        self.counter_collateral(take_profit_price)
    }

    fn notional_size(&self) -> Result<Signed<Notional>> {
        let Self {
            market_type,
            collateral,
            leverage_to_base,
            direction,
            price_point,
            ..
        } = *self;

        calc_notional_size(
            leverage_to_base,
            direction,
            market_type,
            price_point,
            collateral,
        )
    }

    // the take_profit_price here may be:
    // 1. the actual take_profit price the trader is requesting (may be very close to spot price)
    // 2. the calculated minimum take_profit price corresponding to minimum allowed counter-collateral (will be some buffer away from spot price)
    fn counter_collateral(
        &self,
        take_profit_price: TakeProfitPrice,
    ) -> Result<NonZero<Collateral>> {
        let Self {
            market_type,
            collateral,
            price_point,
            leverage_to_base,
            direction,
            ..
        } = *self;

        let notional_size = self.notional_size()?;

        match take_profit_price {
            TakeProfitPrice::PosInfinity => {
                let leverage_to_notional = leverage_to_base
                    .into_signed(direction)
                    .into_notional(market_type);

                let notional_size_in_collateral =
                    leverage_to_notional.checked_mul_collateral(collateral)?;

                match market_type {
                    MarketType::CollateralIsQuote => {
                        Err(MarketError::InvalidInfiniteTakeProfitPrice {
                            market_type,
                            direction,
                        }
                        .into_anyhow())
                    }
                    MarketType::CollateralIsBase => {
                        // In a Collateral-is-base market, infinite max gains are only allowed on
                        // short positions. This is because going short in this market type is betting
                        // on the asset going up (the equivalent of taking a long position in a
                        // Collateral-is-quote market). Note, the error message purposefully describes
                        // this as a "Long" position to keep things clear and consistent for the user.
                        if leverage_to_notional.direction() == DirectionToNotional::Long {
                            return Err(MarketError::InvalidInfiniteTakeProfitPrice {
                                market_type,
                                direction,
                            }
                            .into_anyhow());
                        }

                        NonZero::new(notional_size_in_collateral.abs_unsigned())
                            .context("notional_size_in_collateral is zero")
                    }
                }
            }
            TakeProfitPrice::Finite(take_profit_price) => {
                let take_profit_price = PriceBaseInQuote::from_non_zero(take_profit_price);
                let take_profit_price_notional = take_profit_price.into_notional_price(market_type);

                let counter_collateral = take_profit_price_notional
                    .into_number()
                    .sub(price_point.price_notional.into_number())
                    .mul(notional_size.into_number());

                NonZero::new(Collateral::try_from_number(counter_collateral)?)
                    .context("counter_collateral is zero")
            }
        }
    }

    // the take profit price is max of:
    // 1. a calculated take profit price that would lock up the minimum counter collateral allowed
    // 2. the user-requested take-profit price
    fn capped_take_profit_price(&self) -> Result<TakeProfitPrice> {
        let Self {
            take_profit_price_base,
            market_type,
            collateral,
            leverage_to_base,
            direction,
            config,
            price_point,
        } = *self;

        // minimum allowed counter-collateral
        let min_counter_collateral = price_point
            .notional_to_collateral(self.notional_size()?.abs_unsigned())
            .into_number()
            / config.max_leverage;

        // user requested counter_collateral
        let req_counter_collateral = self
            .counter_collateral(take_profit_price_base)?
            .into_number();

        // counter_collateral at requested price is above min, use requested take_profit price
        if req_counter_collateral > min_counter_collateral {
            Ok(self.take_profit_price_base)
        }
        // counter_collateral at requested price is below min, calculate take_profit price for min counter_collateral
        else {
            TakeProfitFromCounterCollateral {
                market_type,
                collateral,
                counter_collateral: NonZero::new(Collateral::try_from_number(
                    min_counter_collateral,
                )?)
                .context("cannot get non-zero")?,
                leverage_to_base,
                price_point,
                direction,
            }
            .calc()
        }
    }
}
