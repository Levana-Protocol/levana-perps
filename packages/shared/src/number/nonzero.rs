use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use anyhow::Context;
use cosmwasm_std::Decimal256;

use crate::prelude::Signed;

use super::types::{NonZero, UnsignedDecimal};

impl<T: UnsignedDecimal> NonZero<T> {
    /// Get the multiplicative inverse.
    ///
    /// Guaranteed not to fail, since all values here must be great than zero.
    pub fn inverse(self) -> Self {
        NonZero::new(T::from_decimal256(
            Decimal256::one() / self.raw().into_decimal256(),
        ))
        .expect("NonZero::inverse failed but that should be impossible!")
    }
}

impl<T: UnsignedDecimal> Display for NonZero<T> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.raw())
    }
}

impl<T: UnsignedDecimal> TryFrom<u128> for NonZero<T> {
    type Error = anyhow::Error;

    fn try_from(val: u128) -> Result<Self, Self::Error> {
        Signed::<T>::from(val).try_into()
    }
}

impl<T: UnsignedDecimal> TryFrom<u64> for NonZero<T> {
    type Error = anyhow::Error;

    fn try_from(val: u64) -> Result<Self, Self::Error> {
        u128::from(val).try_into()
    }
}

impl<T: UnsignedDecimal> TryFrom<&str> for NonZero<T> {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl<T: UnsignedDecimal> FromStr for NonZero<T> {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Decimal256::from_str(s)
            .ok()
            .map(T::from_decimal256)
            .and_then(NonZero::new)
            .with_context(|| format!("Could not parse a non-zero value from: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::{Number, NumberGtZero};

    use quickcheck::quickcheck;

    quickcheck! {
        fn number_over_zero_roundtrip(num: u128) -> bool {
            let number = (Number::from(num) + Number::ONE).unwrap();
            let number_over_zero = NumberGtZero::try_from(number).unwrap();
            let number2 = Number::from(number_over_zero);
            assert_eq!(number, number2);
            number == number2
        }

        fn negative_fails(num: u128) -> bool {
            NumberGtZero::try_from(-Number::from(num)).unwrap_err();
            true
        }
    }
}
