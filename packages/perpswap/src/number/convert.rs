use anyhow::{Context, Result};
use cosmwasm_std::{Decimal256, Uint128, Uint256};

use super::types::{Signed, UnsignedDecimal};
use super::Number;
use std::fmt::Write;
use std::str::FromStr;

impl Number {
    /// Returns the ratio (nominator / denominator) as a positive Number
    pub fn from_ratio_u256<A: Into<Uint256>, B: Into<Uint256>>(
        numerator: A,
        denominator: B,
    ) -> Self {
        Number::new_positive(Decimal256::from_ratio(numerator, denominator))
    }

    /// Represent as a u128 encoded with given decimal places
    ///
    /// NOTE decimals may be dropped if precision isn't sufficient to represent
    /// all digits completely
    // (also seems to maybe not be sufficient for yocto near?)
    pub fn to_u128_with_precision(&self, precision: u32) -> Option<u128> {
        if self.is_negative() {
            return None;
        }

        // Adjust precision based on given value and chuck in array
        let factor = Decimal256::one().atomics() / Uint256::from_u128(10).pow(precision);
        let raw = self.value().atomics() / factor;

        Uint128::try_from(raw).ok().map(|x| x.into())
    }

    /// helper to get from native currency to Number
    /// e.g. from uusd to UST, as a Decimal
    pub fn from_fixed_u128(amount: u128, places: u32) -> Self {
        (Self::from(amount) / Self::from(10u128.pow(places))).unwrap()
    }

    /// Useful for when Number is used as a PrimaryKey
    /// and is guaranteed to always be positive
    pub fn to_unsigned_key_bytes(&self) -> Option<[u8; 32]> {
        if self.is_positive_or_zero() {
            Some(self.value().atomics().to_be_bytes())
        } else {
            None
        }
    }

    /// Round-tripping with [Self::to_unsigned_key_bytes]
    pub fn from_unsigned_key_bytes(bytes: [u8; 32]) -> Self {
        Number::new_positive(Decimal256::new(Uint256::from_be_bytes(bytes)))
    }
}

//not allowed due to From<Decimal>- impl <T: AsRef<str>> From<T> for Number {
impl TryFrom<&str> for Number {
    type Error = anyhow::Error;

    fn try_from(val: &str) -> Result<Self> {
        Number::from_str(val)
    }
}
impl TryFrom<String> for Number {
    type Error = anyhow::Error;

    fn try_from(val: String) -> Result<Self> {
        Number::from_str(&val)
    }
}

impl<T: UnsignedDecimal> FromStr for Signed<T> {
    type Err = anyhow::Error;

    /// Converts the decimal string to a Number
    /// Possible inputs: "1.23", "1", "000012", "1.123000000", "-1.23"
    /// Disallowed: "", ".23"
    ///
    /// This never performs any kind of rounding.
    /// More than 18 fractional digits, even zeros, result in an error.
    fn from_str(input: &str) -> Result<Self> {
        match input.strip_prefix('-') {
            Some(input) => Decimal256::from_str(input)
                .map(T::from_decimal256)
                .map(Signed::new_negative),
            None => Decimal256::from_str(input)
                .map(T::from_decimal256)
                .map(Signed::new_positive),
        }
        .with_context(|| format!("Unable to parse Number from {input:?}"))
    }
}

impl<T: UnsignedDecimal> From<u128> for Signed<T> {
    fn from(val: u128) -> Self {
        Signed::new_positive(T::from_decimal256(Decimal256::from_ratio(val, 1u32)))
    }
}

impl<T: UnsignedDecimal> From<u64> for Signed<T> {
    fn from(val: u64) -> Self {
        u128::from(val).into()
    }
}

#[cfg(test)]
impl From<f64> for Number {
    fn from(val: f64) -> Self {
        // there is probably a faster way using direct math, e.g. converting to a fraction
        Self::from_str(&format!("{}", val)).unwrap()
    }
}

impl<T: UnsignedDecimal> std::fmt::Display for Signed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_zero() {
            write!(f, "0")
        } else {
            if self.is_negative() {
                f.write_char('-')?;
            }
            write!(f, "{}", self.value())
        }
    }
}
impl<T: UnsignedDecimal> std::fmt::Debug for Signed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_ascii_does_not_panic() {
        Number::try_from("αβ").unwrap_err();
    }

    #[test]
    fn roundtrip_unsigned_bytes() {
        let n = Number::from_str("1.42522").unwrap();
        let bytes = n.to_unsigned_key_bytes().unwrap();
        let n2 = Number::from_unsigned_key_bytes(bytes);
        assert_eq!(n, n2);
    }
}
