use std::ops::{Mul, Sub};

use crate::prelude::*;
use shared::{compat::calc_notional_size, storage::PricePoint};

pub(crate) struct TakeProfitToCounterCollateral<'a> {
    pub(crate) take_profit_trader: TakeProfitTrader,
    pub(crate) market_type: MarketType,
    pub(crate) collateral: NonZero<Collateral>,
    pub(crate) leverage_to_base: LeverageToBase,
    pub(crate) direction: DirectionToBase,
    pub(crate) config: &'a Config,
    pub(crate) price_point: &'a PricePoint,
}

impl<'a> TakeProfitToCounterCollateral<'a> {
    pub(crate) fn calc(&self) -> Result<NonZero<Collateral>> {
        // minimum allowed counter-collateral
        let min_counter_collateral = self
            .price_point
            .notional_to_collateral(self.notional_size()?.abs_unsigned())
            .checked_div_dec(
                self.config
                    .max_leverage
                    .try_into_non_negative_value()
                    .context("Impossible negative max_leverage")?,
            )?;

        // maximum allowed counter-collateral. We have a hard coded minimum
        // leverage of 1, see position_validate_counter_leverage
        let max_counter_collateral = self
            .price_point
            .notional_to_collateral(self.notional_size()?.abs_unsigned());

        // user requested counter_collateral
        let req_counter_collateral = self.counter_collateral(self.take_profit_trader)?;

        let counter_collateral = req_counter_collateral
            .raw()
            .clamp(min_counter_collateral, max_counter_collateral);

        NonZero::new(counter_collateral).context("Calculated counter_collateral is 0")
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
        take_profit_price: TakeProfitTrader,
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
            TakeProfitTrader::PosInfinity => {
                let leverage_to_notional = leverage_to_base
                    .into_signed(direction)
                    .into_notional(market_type)?;

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
            TakeProfitTrader::Finite(take_profit_price) => {
                let take_profit_price = PriceBaseInQuote::from_non_zero(take_profit_price);
                let take_profit_price_notional = take_profit_price.into_notional_price(market_type);

                let counter_collateral = take_profit_price_notional
                    .into_number()
                    .sub(price_point.price_notional.into_number())?
                    .mul(notional_size.into_number())?;

                NonZero::new(Collateral::try_from_number(counter_collateral)?)
                    .context("counter_collateral is zero")
            }
        }
    }
}
