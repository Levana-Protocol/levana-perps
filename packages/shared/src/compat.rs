//! Backwards compatibility helpers
#![allow(missing_docs)]
use std::ops::{Add, Div, Mul};

use crate::storage::{MaxGainsInQuote, PriceBaseInQuote, PricePoint};
use crate::{market_type, prelude::*};

/// Backwards compatible take profit calculation
pub struct BackwardsCompatTakeProfit<'a> {
    pub direction: DirectionToBase,
    pub leverage: LeverageToBase,
    pub market_type: MarketType,
    pub price_point: &'a PricePoint,
    pub max_gains: Option<MaxGainsInQuote>,
    pub take_profit_override: Option<PriceBaseInQuote>,
    pub take_profit: Option<PriceBaseInQuote>,
}

impl <'a> BackwardsCompatTakeProfit<'a> {
    pub fn calc(self) -> Result<PriceBaseInQuote> {
        let BackwardsCompatTakeProfit {
            direction,
            leverage,
            market_type,
            price_point,
            max_gains,
            take_profit_override,
            take_profit,
        } = self;
        match take_profit {
            Some(take_profit) => Ok(take_profit),
            None => match take_profit_override {
                Some(take_profit_override) => Ok(take_profit_override),
                None => match max_gains {
                    Some(max_gains) => {
                        match max_gains {
                            MaxGainsInQuote::Finite(max_gains) => {
                                let direction = match direction {
                                    DirectionToBase::Long => Number::ONE,
                                    DirectionToBase::Short => Number::NEG_ONE,
                                };
                                let max_gains = max_gains.into_decimal256().div(Decimal256::from_ratio(100u32, 1u32)).into_number();
                                let take_profit_price_change = direction.mul(max_gains).div(leverage.into_number());

                                let price = price_point.notional_to_collateral(Notional::one());
                                let take_profit_price = take_profit_price_change.add(Number::ONE).mul(price.into_number());
                                PriceBaseInQuote::try_from_number(take_profit_price)
                            },
                            MaxGainsInQuote::PosInfinity => {
                                // FIXME: what should this be?
                                let price = match direction {
                                    DirectionToBase::Long => Decimal256::MAX,
                                    DirectionToBase::Short => Decimal256::MIN,
                                };

                                Ok(PriceBaseInQuote::from_non_zero(NonZero::try_from_decimal(price).unwrap()))
                            }
                        }
                    },
                    None => Err(MarketError::MissingTakeProfit.into_anyhow()),
                }
            } 
        }
    }
}
