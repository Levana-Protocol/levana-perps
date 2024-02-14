use std::ops::{Add, Div, Mul, Sub};

use shared::{storage::{MarketError, MaxGainsInQuote, PriceBaseInQuote, PricePoint}};
use crate::prelude::*;

pub(crate) struct TakeProfitToCounterCollateral<'a>{
    pub take_profit_price_base: PriceBaseInQuote, 
    pub market_type: MarketType, 
    pub collateral: NonZero<Collateral>,
    pub leverage_to_base: LeverageToBase,
    pub direction: DirectionToBase,
    pub config: &'a Config,
    pub price_point: &'a PricePoint,
} 

enum DebugKind {
    V1,
    V2
}

const DEBUG_KIND: DebugKind = DebugKind::V2;

impl <'a> TakeProfitToCounterCollateral <'a> {

    pub fn calc(&self) -> Result<NonZero<Collateral>> {
        let take_profit_price = self.min_take_profit_price()?;
        let counter_collateral = self.counter_collateral(take_profit_price)?;

        Ok(NonZero::try_from_number(counter_collateral).context("Calculated an invalid counter_collateral")?)
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

        let leverage_to_base = leverage_to_base.into_signed(direction);

        let leverage_to_notional = leverage_to_base.into_notional(market_type);

        let notional_size_in_collateral =
            leverage_to_notional.checked_mul_collateral(collateral)?;
        let notional_size =
            notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x));

        Ok(notional_size.into_number())
    }

    fn counter_collateral(&self, take_profit_price: Number) -> Result<Number> {
        let Self {
            market_type,
            collateral,
            config,
            price_point,
            ..
        } = *self;

        let notional_size = self.notional_size()?;

        let counter_collateral = match DEBUG_KIND {
            // this version is a stab at trying to invert the old take profit calculation
            DebugKind::V1 => {
                let price_notional = price_point.price_notional.into_number();

                take_profit_price
                    .checked_sub(price_notional)?
                    .checked_mul(notional_size)?
            },
            DebugKind::V2 => {
                // this version was from trying to mirror the frontend
                match market_type {
                    MarketType::CollateralIsQuote => {
                        take_profit_price
                            .sub(self.price_notional_in_collateral())
                            .mul(notional_size)
                    },
                    MarketType::CollateralIsBase => {
                        let epsilon = Decimal256::from_ratio(1u32, 1000000u32).into_signed();
                        let take_profit_price_notional = if take_profit_price.approx_lt_relaxed(epsilon) {
                            Number::MAX
                        } else {
                            Number::ONE.div(take_profit_price)
                        };

                        take_profit_price_notional
                            .sub(self.price_notional_in_collateral())
                            .mul(notional_size)
                    }
                }
            }
        };

        println!("TAKE PROFIT PRICE: {}, NOTIONAL SIZE: {} COLLATERAL: {} COUNTER COLLATERAL: {}", take_profit_price, notional_size, collateral, counter_collateral);

        Ok(counter_collateral)
    }

    fn min_take_profit_price(&self) -> Result<Number> {
        let min_max_gains = self.min_max_gains();
        let max_gains_amount = self.max_gains_amount()?;

        let new_take_profit = match max_gains_amount {
            Some(max_gains_amount) if max_gains_amount > min_max_gains => {
                None
            },
            None => {
                None
            }
            Some(max_gains_amount) => {
                let max_gains = max_gains_amount.div(Number::from_ratio_u256(100u32, 1u32));
                let take_profit_price_change = self.direction_number().mul(max_gains).div(self.leverage_to_base.into_number()); 

                Some(take_profit_price_change.add(Number::ONE).mul(self.price_notional_in_collateral()))
            }
        };

        println!("MIN MAX GAINS: {}, MAX GAINS: {:?}, USING TAKE-PROFIT AS-IS: {:?}", min_max_gains, max_gains_amount, new_take_profit.is_none());
        match new_take_profit {
            Some(new_take_profit) => {
                Ok(new_take_profit)
            },
            None => {
                Ok(self.take_profit_price_base.into_number())
            }
        }
    }

    fn min_max_gains(&self) -> Number {
        match self.market_type {
            MarketType::CollateralIsQuote => {
                self
                    .leverage_to_base
                    .into_number()
                    .div(self.config.max_leverage)
                    .mul(Number::from_ratio_u256(100u32, 1u32))
                    .abs_unsigned()
                    .ceil()
                    .into_number()
            },
            MarketType::CollateralIsBase => {

                Number::NEG_ONE
                    .div(Number::ONE.sub(self.direction_number().mul(self.config.max_leverage)))
                    .mul(self.leverage_to_base.into_number())
                    .mul(self.direction_number())
                    .mul(Number::from_ratio_u256(100u32, 1u32))
                    .abs_unsigned()
                    .ceil()
                    .into_number()

            }
        }.div(Number::from_ratio_u256(100u32, 1u32))
        
    }

    fn max_gains_amount(&self) -> Result<Option<Number>> {
        let notional_size = self.notional_size()?;
        let counter_collateral = self.counter_collateral(self.take_profit_price_base.into_number())?;
        let active_collateral = self.collateral.into_number();


        let max_gains = match self.market_type {
            MarketType::CollateralIsQuote => {
                counter_collateral.div(active_collateral)
            },
            MarketType::CollateralIsBase => {
                let take_profit_collateral = active_collateral.add(counter_collateral);

                let take_profit_price = self.price_notional_in_collateral().add(counter_collateral.div(notional_size));

                let epsilon = Decimal256::from_ratio(1u32, 1000000u32).into_signed();

                if take_profit_price.approx_lt_relaxed(epsilon) {
                    return Ok(None); 
                } 

                let take_profit_in_notional = take_profit_collateral.div(take_profit_price);

                let active_collateral_in_notional = self.price_point.collateral_to_notional(Collateral::from_decimal256(active_collateral.abs_unsigned())).into_number();

                take_profit_in_notional.sub(active_collateral_in_notional).div(active_collateral_in_notional)
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
        self.price_point.notional_to_collateral(Notional::one()).into_number()
    }

}