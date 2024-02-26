//! Backwards compatibility helpers
// this file should be completely deleted when max_gains is deleted
#![allow(missing_docs)]

use crate::prelude::*;
use crate::storage::{MaxGainsInQuote, PricePoint};

/// Backwards compatible take profit calculation
///
/// To set "infinite max gains", with new API style
/// both max_gains should be None and take_profit is None
/// (which will result in the calculated take_profit itself to be None
/// and then this whole struct can be removed along with max_gains)
pub struct BackwardsCompatTakeProfit<'a> {
    pub collateral: NonZero<Collateral>,
    pub direction: DirectionToBase,
    pub leverage: LeverageToBase,
    pub market_type: MarketType,
    pub price_point: &'a PricePoint,
    pub max_gains: MaxGainsInQuote,
    pub take_profit: Option<PriceBaseInQuote>,
}

impl<'a> BackwardsCompatTakeProfit<'a> {
    pub fn calc(self) -> Result<TakeProfitPrice> {
        let BackwardsCompatTakeProfit {
            collateral,
            direction,
            leverage,
            market_type,
            price_point,
            max_gains,
            take_profit,
        } = self;
        match take_profit {
            Some(take_profit) => Ok(TakeProfitPrice::Finite(take_profit.into_non_zero())),
            None => match max_gains {
                MaxGainsInQuote::PosInfinity => Ok(TakeProfitPrice::PosInfinity),
                MaxGainsInQuote::Finite(_) => {
                    let leverage_to_notional =
                        leverage.into_signed(direction).into_notional(market_type);

                    let notional_size_in_collateral =
                        leverage_to_notional.checked_mul_collateral(collateral)?;

                    let counter_collateral = max_gains.calculate_counter_collateral(
                        market_type,
                        collateral,
                        notional_size_in_collateral,
                        leverage_to_notional,
                    )?;

                    TakeProfitFromCounterCollateral {
                        counter_collateral,
                        market_type,
                        collateral,
                        leverage_to_base: self.leverage,
                        price_point,
                        direction,
                    }
                    .calc()
                }
            },
        }
    }
}

// just a local helper
struct TakeProfitFromCounterCollateral<'a> {
    pub market_type: MarketType,
    pub collateral: NonZero<Collateral>,
    pub counter_collateral: NonZero<Collateral>,
    pub leverage_to_base: LeverageToBase,
    pub price_point: &'a PricePoint,
    pub direction: DirectionToBase,
}
impl<'a> TakeProfitFromCounterCollateral<'a> {
    pub fn calc(&self) -> Result<TakeProfitPrice> {
        let Self {
            market_type,
            collateral,
            counter_collateral,
            leverage_to_base,
            price_point,
            direction,
        } = self;

        let notional_size = calc_notional_size(
            *leverage_to_base,
            *direction,
            *market_type,
            price_point,
            *collateral,
        )?;

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
            Some(price) => Ok(TakeProfitPrice::Finite(price.into_base_price(*market_type).into_non_zero())),
            None =>
            match market_type {
                // Infinite max gains results in a notional take profit price of 0
                MarketType::CollateralIsBase => Ok(TakeProfitPrice::PosInfinity),
                MarketType::CollateralIsQuote => Err(anyhow!("Calculated a take profit price of {take_profit_price_raw} in a collateral-is-quote market. Spot notional price: {}. Counter collateral: {}. Notional size: {}.", price_point.price_notional, self.counter_collateral,notional_size)),
            }
        }
    }
}

pub fn calc_notional_size(
    leverage: LeverageToBase,
    direction: DirectionToBase,
    market_type: MarketType,
    price_point: &PricePoint,
    collateral: NonZero<Collateral>,
) -> Result<Signed<Notional>> {
    let leverage_to_base = leverage.into_signed(direction);

    let leverage_to_notional = leverage_to_base.into_notional(market_type);

    let notional_size_in_collateral = leverage_to_notional.checked_mul_collateral(collateral)?;

    Ok(notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x)))
}
