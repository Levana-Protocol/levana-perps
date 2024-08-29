use msg::contracts::market::config::Config;
use msg::prelude::*;
use msg::token::Token;
use once_cell::sync::Lazy;
use proptest::prelude::*;
use std::ops::{Div, Mul, Sub};
use std::ops::{Range, RangeInclusive};

pub fn max_gains_strategy(
    direction: DirectionToBase,
    leverage_base: LeverageToBase,
    market_type: MarketType,
    config: &Config,
) -> impl Strategy<Value = MaxGainsInQuote> {
    let max_leverage_base =
        LeverageToBase::try_from(config.max_leverage.to_string().as_str()).unwrap();

    let leverage_notional: f32 = leverage_base
        .into_signed(direction)
        .into_notional(market_type)
        .unwrap()
        .into_number()
        .to_string()
        .parse::<f32>()
        .unwrap()
        .abs();

    let max_leverage_notional: f32 = max_leverage_base
        .into_signed(direction)
        .into_notional(market_type)
        .unwrap()
        .into_number()
        .to_string()
        .parse::<f32>()
        .unwrap()
        .abs();

    let max_gains_can_be_infinite =
        direction == DirectionToBase::Long && market_type == MarketType::CollateralIsBase;

    let finite = {
        let range: RangeInclusive<f32> = match market_type {
            MarketType::CollateralIsQuote => match direction {
                DirectionToBase::Long => {
                    let min = leverage_notional / max_leverage_notional;
                    let max = leverage_notional;
                    min..=max
                }
                DirectionToBase::Short => {
                    let min = leverage_notional / max_leverage_notional;
                    let max = leverage_notional;
                    min..=max
                }
            },
            MarketType::CollateralIsBase => calculate_max_gains_range_collateral_is_base(
                max_leverage_base,
                leverage_base,
                direction,
            ),
        };

        range.prop_map(|x| {
            MaxGainsInQuote::Finite(
                x.to_string()
                    .parse()
                    .unwrap_or_else(|_| panic!("{x} should be a valid max gains!")),
            )
        })
    };

    if max_gains_can_be_infinite {
        prop_oneof![
            proptest::strategy::Just(MaxGainsInQuote::PosInfinity),
            finite
        ]
        .boxed()
    } else {
        finite.boxed()
    }
}

pub fn calculate_max_gains_range_collateral_is_base(
    max_leverage_base: LeverageToBase,
    leverage: LeverageToBase,
    direction_to_base: DirectionToBase,
) -> RangeInclusive<f32> {
    let max_leverage_base: f32 = max_leverage_base.to_string().parse().unwrap();
    let leverage: f32 = leverage.to_string().parse().unwrap();
    let direction = if direction_to_base == DirectionToBase::Long {
        1.0
    } else {
        -1.0
    };

    let min = -(1.0f32
        .div(1.0.sub(direction.mul(max_leverage_base)))
        .mul(leverage)
        .mul(direction));

    let max = match direction_to_base {
        DirectionToBase::Long => {
            -(1.0
                .div(1.0.sub(direction.div(0.9)))
                .mul(leverage)
                .mul(direction))
        }
        DirectionToBase::Short => -(0.5.mul(leverage).mul(direction)),
    };

    min..=max
}

pub static MOCK_TOKEN: Lazy<Token> = Lazy::new(|| Token::Native {
    denom: "foo".to_string(),
    decimal_places: 6,
});

// given min/max as simple decimal strings, gives us back a range
// which is valid for round-tripping and won't have precision issues
pub fn token_range_u128(min: &str, max: &str) -> Range<u128> {
    let min = MOCK_TOKEN.into_u128(min.parse().unwrap()).unwrap().unwrap();
    let max = MOCK_TOKEN.into_u128(max.parse().unwrap()).unwrap().unwrap();
    min..max
}
