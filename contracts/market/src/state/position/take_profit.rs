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

    // this version is a stab at trying to invert the old take profit calculation
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
        let take_profit_price = self.min_take_profit_price();

        let price_notional = price_point.price_notional.into_number();

        let notional_size = calculate_notional_size(
            price_point,
            market_type,
            collateral,
            leverage_to_base,
            direction,
        )?;

        let counter_collateral = take_profit_price
            .checked_sub(price_notional)?
            .checked_mul(notional_size.into_number())?;

        println!("TAKE PROFIT PRICE: {}, PRICE NOTIONAL: {} NOTIONAL SIZE: {} COLLATERAL: {} COUNTER COLLATERAL: {}", take_profit_price, price_notional, notional_size, collateral, counter_collateral);
        Ok(NonZero::try_from_number(counter_collateral).context("Calculated an invalid counter_collateral")?)
    }

    // this version was from trying to mirror the frontend
    pub fn calc_v2(&self) -> Result<NonZero<Collateral>> {
        let Self {
            take_profit_price_base,
            market_type,
            collateral,
            leverage_to_base,
            direction,
            config,
            price_point,
        } = *self;
        let take_profit_price = self.min_take_profit_price();

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

        println!("TAKE PROFIT PRICE: {}, PRICE NOTIONAL: {} NOTIONAL SIZE: {} COLLATERAL: {} COUNTER COLLATERAL: {}", take_profit_price, price_notional_in_collateral, notional_size, collateral, counter_collateral);

        Ok(NonZero::try_from_number(counter_collateral).context("Calculated an invalid counter_collateral")?)

    }

    fn min_take_profit_price(&self) -> Number {
        self.take_profit_price_base.into_number()
        // TODO - need to implement this sorta thing? copied from frontend:
        // const minMaxGains = calculateMaxGainsRange({
        //     direction: position.directionToBase,
        //     maxLeverage: marketConfig.maxLeverage,
        //     leverage: position.leverage,
        //     marketType: marketConfig.type,
        //   }).min.divide(100)
        
        //   const newTakeProfit = inferredInputValue(props.takeProfitAmount)
        
        //   const newMaxGainsAmount = calculateMaxGainsFromDependencies({
        //     collateral: position.activeCollateral,
        //     marketPrice: marketPrice,
        //     direction: position.directionToBase,
        //     leverage: position.leverage,
        //     takeProfitPrice: newTakeProfit,
        //     maxLeverage: marketConfig.maxLeverage,
        //     allowNegative: false,
        //   }).maxGains.divide(100)
        
        //   const maxGainsPrice = (() => {
        //     if (newMaxGainsAmount.isGreaterThan(minMaxGains)) {
        //       return newTakeProfit
        //     } else {
        //       return calculateTakeProfitPrice({
        //         direction: position.directionToBase,
        //         leverage: position.leverage.toString(),
        //         maxGains: minMaxGains.times(100),
        //         marketPrice: marketPrice,
        //       }).takeProfitPrice
        //     }
        //   })()
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