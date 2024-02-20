//! Backwards compatibility helpers
#![allow(missing_docs)]

use crate::storage::{MaxGainsInQuote, PriceBaseInQuote, PricePoint};
use crate::prelude::*;

/// Backwards compatible take profit calculation
pub struct BackwardsCompatTakeProfit<'a> {
    pub collateral: NonZero<Collateral>,
    pub direction: DirectionToBase,
    pub leverage: LeverageToBase,
    pub market_type: MarketType,
    pub price_point: &'a PricePoint,
    pub max_gains: Option<MaxGainsInQuote>,
    pub take_profit_override: Option<PriceBaseInQuote>,
    pub take_profit: Option<PriceBaseInQuote>,
}

impl <'a> BackwardsCompatTakeProfit<'a> {
    pub fn calc(self) -> Result<Option<PriceBaseInQuote>> {
        let BackwardsCompatTakeProfit {
            collateral,
            direction,
            leverage,
            market_type,
            price_point,
            max_gains,
            take_profit_override,
            take_profit,
        } = self;
        match take_profit {
            Some(take_profit) => Ok(Some(take_profit)),
            None => match take_profit_override {
                Some(take_profit_override) => Ok(Some(take_profit_override)),
                None => match max_gains {
                    Some(max_gains) => {
                        let leverage_to_notional = leverage.into_signed(direction).into_notional(market_type);

                        let notional_size_in_collateral =
                            leverage_to_notional.checked_mul_collateral(collateral)?;

                        let counter_collateral = max_gains.calculate_counter_collateral(market_type, collateral, notional_size_in_collateral, leverage_to_notional)?;



                        TakeProfitFromCounterCollateral {
                            counter_collateral,
                            market_type,
                            collateral,
                            leverage_to_base: self.leverage,
                            price_point,
                            direction,
                        }.calc()
                    },
                    None => Ok(None)
                }
            } 
        }
    }
}

struct TakeProfitFromCounterCollateral<'a>{
    pub market_type: MarketType,
    pub collateral: NonZero<Collateral>,
    pub counter_collateral: NonZero<Collateral>,
    pub leverage_to_base: LeverageToBase,
    pub price_point: &'a PricePoint,
    pub direction: DirectionToBase,
} 
impl <'a> TakeProfitFromCounterCollateral <'a> {
    pub fn calc(&self) -> Result<Option<PriceBaseInQuote>> {
        let Self {
            market_type,
            collateral,
            counter_collateral,
            leverage_to_base,
            price_point,
            direction,
        } = self;

        let notional_size = calc_notional_size(*leverage_to_base, *direction, *market_type, price_point, *collateral)?;


        let take_profit_price_raw = price_point.price_notional.into_number().checked_add(
            counter_collateral
                .into_number()
                .checked_div(notional_size.into_number())?,
        )?;

        let take_profit_price = if take_profit_price_raw.approx_eq(Number::ZERO) {
            None
        } else {
            debug_assert!(
                take_profit_price_raw.is_positive_or_zero(),
                "There should never be a calculated take profit price which is negative. In production, this is treated as 0 to indicate infinite max gains."
            );
            Price::try_from_number(take_profit_price_raw).ok()
        };

        match take_profit_price {
            Some(price) => Ok(Some(price.into_base_price(*market_type))),
            None =>
            match market_type {
                // Infinite max gains results in a notional take profit price of 0
                MarketType::CollateralIsBase => Ok(None),
                MarketType::CollateralIsQuote => Err(anyhow!("Calculated a take profit price of {take_profit_price_raw} in a collateral-is-quote market. Spot notional price: {}. Counter collateral: {}. Notional size: {}.", price_point.price_notional, self.counter_collateral,notional_size)),
            }
        }
    }
}

pub fn calc_notional_size(leverage: LeverageToBase, direction: DirectionToBase, market_type: MarketType, price_point: &PricePoint, collateral: NonZero<Collateral>) -> Result<Signed<Notional>> {
    let leverage_to_base = leverage.into_signed(direction);

    let leverage_to_notional = leverage_to_base.into_notional(market_type);

    let notional_size_in_collateral =
        leverage_to_notional.checked_mul_collateral(collateral)?;

    Ok(notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x)))
}