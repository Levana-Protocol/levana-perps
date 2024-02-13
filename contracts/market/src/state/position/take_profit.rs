use std::ops::{Div, Mul, Sub};

use shared::{direction, storage::{MarketError, MaxGainsInQuote, PriceBaseInQuote, PricePoint}};
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
impl <'a> TakeProfitToCounterCollateral <'a> {
    pub fn calc(&self) -> Result<NonZero<Collateral>> {
        let Self {
            take_profit_price_base,
            market_type,
            collateral,
            leverage_to_base,
            direction,
            config,
            price_point,
        } = *self;
        // TODO - switch take profit price if lower than min max-gains price?
        let take_profit_price = take_profit_price_base.into_number();

        let notional_size = calculate_notional_size(
            price_point,
            market_type,
            collateral,
            leverage_to_base,
            direction,
        )?;

        let min_counter_collateral = price_point.notional_to_collateral(notional_size.abs_unsigned()).into_number().div(config.max_leverage);

        let price_notional_in_collateral = price_point.notional_to_collateral(Notional::one()).into_number();
        let counter_collateral = match market_type {
            MarketType::CollateralIsQuote => {
                take_profit_price
                    .sub(price_notional_in_collateral)
                    .mul(notional_size.into_number())
            },
            MarketType::CollateralIsBase => {
                let epsilon = Decimal256::from_ratio(1u32, 1000000u32).into_signed();
                let take_profit_price_notional = if take_profit_price.approx_lt_relaxed(epsilon) {
                    Number::MAX
                } else {
                    Number::ONE.div(take_profit_price)
                };

                take_profit_price_notional
                    .sub(price_notional_in_collateral)
                    .mul(notional_size.into_number())
            }
        };

        Ok(NonZero::try_from_number(counter_collateral).context("Calculated an invalid counter_collateral")?)

    }
}

fn calculate_notional_size(
    price_point: &PricePoint,
    market_type: MarketType,
    collateral: NonZero<Collateral>,
    leverage_to_base: LeverageToBase,
    direction: DirectionToBase,
) -> Result<Signed<Notional>> {
    let leverage_to_base = leverage_to_base.into_signed(direction);

    let leverage_to_notional = leverage_to_base.into_notional(market_type);

    let notional_size_in_collateral =
        leverage_to_notional.checked_mul_collateral(collateral)?;
    let notional_size =
        notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x));

    Ok(notional_size)
}