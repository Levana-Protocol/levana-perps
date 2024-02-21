use std::ops::{Add, Div, Mul, Sub};

use crate::prelude::*;
use shared::{
    compat::calc_notional_size,
    storage::{MaxGainsInQuote, PriceBaseInQuote, PricePoint},
};

pub(crate) struct TakeProfitToCounterCollateral<'a> {
    pub take_profit_price_base: Option<PriceBaseInQuote>,
    pub market_type: MarketType,
    pub collateral: NonZero<Collateral>,
    pub leverage_to_base: LeverageToBase,
    pub direction: DirectionToBase,
    pub config: &'a Config,
    pub price_point: &'a PricePoint,
}

impl<'a> TakeProfitToCounterCollateral<'a> {
    pub fn calc(&self) -> Result<NonZero<Collateral>> {
        let take_profit_price = self.min_take_profit_price()?;
        let counter_collateral = self.counter_collateral(take_profit_price)?;

        NonZero::try_from_number(counter_collateral)
            .context("Calculated an invalid counter_collateral from take_profit")
    }

    fn notional_size(&self) -> Result<Number> {
        let Self {
            market_type,
            collateral,
            leverage_to_base,
            direction,
            price_point,
            ..
        } = *self;

        Ok(calc_notional_size(
            leverage_to_base,
            direction,
            market_type,
            price_point,
            collateral,
        )?
        .into_number())
    }

    // the take_profit_price here is passed in since it may be the "min max gains" price
    // or the user-supplied take profit price (i.e. self.take_profit_price_base)
    fn counter_collateral(&self, take_profit_price: Option<Number>) -> Result<Number> {
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
            None => {
                let leverage_to_notional = leverage_to_base
                    .into_signed(direction)
                    .into_notional(market_type);

                let notional_size_in_collateral =
                    leverage_to_notional.checked_mul_collateral(collateral)?;

                MaxGainsInQuote::PosInfinity
                    .calculate_counter_collateral(
                        market_type,
                        collateral,
                        notional_size_in_collateral,
                        leverage_to_notional,
                    )
                    .map(|x| x.into_number())
            }
            Some(take_profit_price) => {
                // this version was from trying to mirror the frontend
                // TODO - clean this up, is probably a method on price_point or similar
                // should not need the epsilon at all which was taken from the frontend
                let take_profit_price_notional = match market_type {
                    MarketType::CollateralIsQuote => take_profit_price,
                    MarketType::CollateralIsBase => {
                        let epsilon = Decimal256::from_ratio(1u32, 1000000u32).into_signed();
                        if take_profit_price.approx_lt_relaxed(epsilon) {
                            Number::MAX
                        } else {
                            Number::ONE.div(take_profit_price)
                        }
                    }
                };

                let counter_collateral = take_profit_price_notional
                    .sub(price_point.price_notional.into_number())
                    .mul(notional_size);

                Ok(counter_collateral)
            }
        }
    }

    fn min_take_profit_price(&self) -> Result<Option<Number>> {
        let min_max_gains = self.min_max_gains();
        let max_gains_amount = self.max_gains_amount()?;

        let new_take_profit = match max_gains_amount {
            Some(max_gains_amount) if max_gains_amount > min_max_gains => None,
            None => None,
            Some(max_gains_amount) => {
                let max_gains = max_gains_amount.div(Number::from_ratio_u256(100u32, 1u32));
                let take_profit_price_change = self
                    .direction_number()
                    .mul(max_gains)
                    .div(self.leverage_to_base.into_number());

                Some(
                    take_profit_price_change
                        .add(Number::ONE)
                        .mul(self.price_notional_in_collateral()),
                )
            }
        };

        match new_take_profit {
            Some(new_take_profit) => Ok(Some(new_take_profit)),
            None => Ok(self.take_profit_price_base.map(|x| x.into_number())),
        }
    }

    fn min_max_gains(&self) -> Number {
        match self.market_type {
            MarketType::CollateralIsQuote => self
                .leverage_to_base
                .into_number()
                .div(self.config.max_leverage)
                .mul(Number::from_ratio_u256(100u32, 1u32))
                .abs_unsigned()
                .ceil()
                .into_number(),
            MarketType::CollateralIsBase => Number::NEG_ONE
                .div(Number::ONE.sub(self.direction_number().mul(self.config.max_leverage)))
                .mul(self.leverage_to_base.into_number())
                .mul(self.direction_number())
                .mul(Number::from_ratio_u256(100u32, 1u32))
                .abs_unsigned()
                .ceil()
                .into_number(),
        }
        .div(Number::from_ratio_u256(100u32, 1u32))
    }

    fn max_gains_amount(&self) -> Result<Option<Number>> {
        let notional_size = self.notional_size()?;
        let counter_collateral =
            self.counter_collateral(self.take_profit_price_base.map(|x| x.into_number()))?;
        let active_collateral = self.collateral.into_number();

        let max_gains = match self.market_type {
            MarketType::CollateralIsQuote => counter_collateral.div(active_collateral),
            MarketType::CollateralIsBase => {
                let take_profit_collateral = active_collateral.add(counter_collateral);

                let take_profit_price = self
                    .price_notional_in_collateral()
                    .add(counter_collateral.div(notional_size));

                let epsilon = Decimal256::from_ratio(1u32, 1000000u32).into_signed();

                if take_profit_price.approx_lt_relaxed(epsilon) {
                    return Ok(None);
                }

                let take_profit_in_notional = take_profit_collateral.div(take_profit_price);

                let active_collateral_in_notional = self
                    .price_point
                    .collateral_to_notional(Collateral::from_decimal256(
                        active_collateral.abs_unsigned(),
                    ))
                    .into_number();

                take_profit_in_notional
                    .sub(active_collateral_in_notional)
                    .div(active_collateral_in_notional)
            }
        };

        Ok(Some(max_gains.mul(Number::from_ratio_u256(100u32, 1u32))))
    }

    fn direction_number(&self) -> Number {
        match self.direction {
            DirectionToBase::Long => Number::ONE,
            DirectionToBase::Short => Number::NEG_ONE,
        }
    }

    fn price_notional_in_collateral(&self) -> Number {
        self.price_point
            .notional_to_collateral(Notional::one())
            .into_number()
    }
}
