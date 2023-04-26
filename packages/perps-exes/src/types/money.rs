use msg::prelude::*;

pub fn notional_max_gain(gain_percentage: MaxGainsInQuote) -> MaxGainsInQuote {
    match gain_percentage {
        MaxGainsInQuote::Finite(number) => MaxGainsInQuote::Finite(
            NonZero::new(number.into_decimal256() / Decimal256::from_str("100").unwrap()).unwrap(),
        ),
        MaxGainsInQuote::PosInfinity => MaxGainsInQuote::PosInfinity,
    }
}
