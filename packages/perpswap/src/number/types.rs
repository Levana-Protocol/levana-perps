//! This module provides raw data types and just enough functionality to
//! construct them. It exports smart contructors to the rest of the crate to
//! ensure that invariants are never violated.

use anyhow::{Context, Result};
use cosmwasm_std::{Decimal256, OverflowError, Uint128, Uint256};
use std::{
    fmt::Display,
    ops::{Add, Sub},
    str::FromStr,
};

/// needed to get around the orphan rule
#[cfg(feature = "arbitrary")]
pub fn arbitrary_decimal_256(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Decimal256> {
    let bytes: [u8; 32] = u.arbitrary()?;
    let value = Uint256::new(bytes);
    Ok(Decimal256::new(value))
}

/// needed to get around the orphan rule
#[cfg(feature = "arbitrary")]
pub fn arbitrary_decimal_256_option(
    u: &mut arbitrary::Unstructured,
) -> arbitrary::Result<Option<Decimal256>> {
    let bytes: Option<[u8; 32]> = u.arbitrary()?;
    Ok(bytes.map(|bytes| {
        let value = Uint256::new(bytes);
        Decimal256::new(value)
    }))
}

/// Generalizes any newtype wrapper around a [Decimal256].
pub trait UnsignedDecimal:
    Display
    + std::fmt::Debug
    + serde::Serialize
    + serde::de::DeserializeOwned
    + Copy
    + Ord
    + FromStr
    + Default
{
    /// Convert into the underlying [Decimal256].
    fn into_decimal256(self) -> Decimal256;

    /// Convert from a [Decimal256].
    fn from_decimal256(src: Decimal256) -> Self;

    /// Check if the underlying value is 0.
    fn is_zero(&self) -> bool {
        self.into_decimal256().is_zero()
    }

    /// Add two values together
    fn checked_add(self, rhs: Self) -> Result<Self, OverflowError> {
        self.into_decimal256()
            .checked_add(rhs.into_decimal256())
            .map(Self::from_decimal256)
    }

    /// Try to add a signed value to this, erroring if it results in a negative result.
    fn checked_add_signed(self, rhs: Signed<Self>) -> Result<Self> {
        self.into_signed()
            .checked_add(rhs)?
            .try_into_non_negative_value()
            .with_context(|| format!("{self} + {rhs}"))
    }

    /// Subtract two values
    fn checked_sub(self, rhs: Self) -> Result<Self, OverflowError> {
        self.into_decimal256()
            .checked_sub(rhs.into_decimal256())
            .map(Self::from_decimal256)
    }

    /// Try to convert from a general purpose [Number]
    fn try_from_number(Signed { value, negative }: Signed<Decimal256>) -> anyhow::Result<Self> {
        if negative {
            Err(anyhow::anyhow!(
                "try_from_number: received a negative value"
            ))
        } else {
            Ok(Self::from_decimal256(value))
        }
    }

    /// convert into a general purpose [Number]
    fn into_number(self) -> Signed<Decimal256> {
        Signed::new_positive(self.into_decimal256())
    }

    /// Convert into a positive [Signed] value.
    fn into_signed(self) -> Signed<Self> {
        Signed::new_positive(self)
    }

    /// The value 0
    fn zero() -> Self {
        Self::from_decimal256(Decimal256::zero())
    }

    /// The value 2
    fn two() -> Self {
        Self::from_decimal256(Decimal256::from_atomics(2u128, 0).unwrap())
    }

    /// Difference between two values
    fn diff(self, rhs: Self) -> Self {
        Self::from_decimal256(if self > rhs {
            self.into_decimal256() - rhs.into_decimal256()
        } else {
            rhs.into_decimal256() - self.into_decimal256()
        })
    }

    /// Is the delta between these less than the epsilon value?
    ///
    /// Epsilon is `10^-7`
    fn approx_eq(self, rhs: Self) -> bool {
        self.diff(rhs).into_decimal256() < Decimal256::from_ratio(1u32, 10_000_000u32)
    }

    // Note: we do _not_ include multiplication and division, since some operations
    // (like multiplying two Collateral values together) are non-sensical.
}

impl UnsignedDecimal for Decimal256 {
    fn into_decimal256(self) -> Decimal256 {
        self
    }

    fn from_decimal256(src: Decimal256) -> Self {
        src
    }
}

macro_rules! unsigned {
    ($t:tt) => {
        // Avoid using cw_serde because Decimal256 has a bad Debug impl
        #[derive(
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Clone,
            Copy,
            Default,
            serde::Serialize,
            serde::Deserialize,
            schemars::JsonSchema,
        )]
        /// Unsigned value
        pub struct $t(Decimal256);

        impl $t {
            /// Zero value
            pub const fn zero() -> Self {
                Self(Decimal256::zero())
            }

            /// One value
            pub const fn one() -> Self {
                Self(Decimal256::one())
            }
        }

        impl UnsignedDecimal for $t {
            fn into_decimal256(self) -> Decimal256 {
                self.0
            }

            fn from_decimal256(src: Decimal256) -> Self {
                Self(src)
            }
        }

        impl Display for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($t), self.0)
            }
        }

        impl FromStr for $t {
            type Err = anyhow::Error;

            fn from_str(s: &str) -> Result<Self> {
                parse_decimal256(s).map(Self::from_decimal256)
            }
        }

        impl TryFrom<&str> for $t {
            type Error = anyhow::Error;

            fn try_from(value: &str) -> Result<Self> {
                value.parse()
            }
        }

        impl TryFrom<String> for Signed<$t> {
            type Error = anyhow::Error;

            fn try_from(value: String) -> Result<Self> {
                value.parse()
            }
        }

        impl TryFrom<&str> for Signed<$t> {
            type Error = anyhow::Error;

            fn try_from(value: &str) -> Result<Self> {
                value.parse()
            }
        }

        impl TryFrom<String> for $t {
            type Error = anyhow::Error;

            fn try_from(value: String) -> Result<Self> {
                value.parse()
            }
        }

        impl Add for $t {
            type Output = anyhow::Result<Self, OverflowError>;

            fn add(self, rhs: Self) -> Self::Output {
                Ok(Self(self.0.checked_add(rhs.0)?))
            }
        }

        impl Sub for $t {
            type Output = anyhow::Result<Self, OverflowError>;

            fn sub(self, rhs: Self) -> Self::Output {
                Ok(Self(self.0.checked_sub(rhs.0)?))
            }
        }

        impl From<u64> for $t {
            fn from(src: u64) -> Self {
                u128::from(src).into()
            }
        }

        impl From<u128> for $t {
            fn from(src: u128) -> Self {
                Self::from_decimal256(Decimal256::from_ratio(src, 1u32))
            }
        }

        #[cfg(feature = "arbitrary")]
        impl<'a> arbitrary::Arbitrary<'a> for $t {
            fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
                Ok(Self::from_decimal256(arbitrary_decimal_256(u)?))
            }
        }

        impl $t {
            /// Floor the current value with given decimal precision
            pub fn floor_with_precision(&self, precision: u32) -> Self {
                // Adjust precision based on given value and chuck in array
                let factor = Decimal256::one().atomics() / Uint256::from_u128(10).pow(precision);
                let raw = self.0.atomics() / factor * factor;

                Self(Decimal256::new(raw))
            }
        }
    };
}

fn parse_decimal256(s: &str) -> Result<Decimal256> {
    s.parse()
        .with_context(|| format!("Unable to parse unsigned decimal from {s}"))
}

unsigned!(Collateral);
unsigned!(Notional);
unsigned!(Base);
unsigned!(Quote);
unsigned!(Usd);
unsigned!(LpToken);
unsigned!(FarmingToken);
unsigned!(LvnToken);
unsigned!(LockdropShares);

/// Wrap up any [UnsignedDecimal] to provide negative values too.
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct Signed<T> {
    value: T,
    /// Invariant: must always be false if value is 0
    negative: bool,
}

impl<T: UnsignedDecimal> Default for Signed<T> {
    fn default() -> Self {
        Signed {
            value: T::default(),
            negative: false,
        }
    }
}

impl<T> From<T> for Signed<T> {
    fn from(value: T) -> Self {
        Signed {
            value,
            negative: false,
        }
    }
}

impl<T: UnsignedDecimal> Signed<T> {
    pub(crate) fn value(self) -> T {
        self.value
    }

    /// Strictly less than 0, returns false on 0
    pub fn is_negative(&self) -> bool {
        self.negative
    }

    /// create a new positive Number with the given value
    pub(crate) fn new_positive(value: T) -> Self {
        Self {
            value,
            negative: false,
        }
    }

    /// create a new negative Number with the given value
    pub(crate) fn new_negative(value: T) -> Self {
        Self {
            value,
            negative: !value.is_zero(),
        }
    }

    /// Convert into a general purpose [Number].
    pub fn into_number(self) -> Signed<Decimal256> {
        Signed {
            value: self.value.into_decimal256(),
            negative: self.negative,
        }
    }

    /// convert from a general purpose [Number].
    pub fn from_number(src: Signed<Decimal256>) -> Self {
        Signed {
            value: T::from_decimal256(src.value),
            negative: src.negative,
        }
    }

    /// The value 0
    pub fn zero() -> Self {
        Signed::new_positive(T::zero())
    }

    /// The value 2
    pub fn two() -> Self {
        Signed::new_positive(T::two())
    }

    /// If the value is positive or zero, return the inner `T`. Otherwise return `None`.
    pub fn try_into_non_negative_value(self) -> Option<T> {
        if self.is_negative() {
            None
        } else {
            Some(self.value())
        }
    }

    /// Try to convert into a non-zero value
    pub fn try_into_non_zero(self) -> Option<NonZero<T>> {
        self.try_into_non_negative_value().and_then(NonZero::new)
    }
}

impl Signed<Decimal256> {
    /// The maximum allowed
    pub const MAX: Self = Self {
        value: Decimal256::MAX,
        negative: false,
    };

    /// The minimum allowed
    pub const MIN: Number = Number {
        value: Decimal256::MAX,
        negative: true,
    };

    /// 1 as a Number
    pub const ONE: Number = Number {
        value: Decimal256::one(),
        negative: false,
    };

    /// -1 as a Number
    pub const NEG_ONE: Number = Number {
        value: Decimal256::one(),
        negative: true,
    };

    /// 0 as a Number
    pub const ZERO: Number = Number {
        value: Decimal256::zero(),
        negative: false,
    };

    /// Default epsilon used for approximate comparisons
    // dev hint: if you want to get a new value
    // print out a Number.abs_value_as_array()
    // then just set that here
    pub const EPS_E7: Number = Number {
        // 18 digits precision - 7 digits == 11 zeros
        value: Decimal256::raw(100_000_000_000),
        negative: false,
    };

    /// An alternate epsilon that can be used for approximate comparisons
    pub const EPS_E6: Number = Number {
        value: Decimal256::raw(1_000_000_000_000),
        negative: false,
    };

    /// Another alternate epsilon that can be used for approximate comparisons
    /// where the rounding error is due to the Decimal256 representation
    /// as opposed to, say, token precision
    pub const EPS_E17: Number = Number {
        // 18 digits precision - 17 digits == 1 zero
        value: Decimal256::raw(10),
        negative: false,
    };
}

impl<T: UnsignedDecimal> std::ops::Neg for Signed<T> {
    type Output = Self;

    fn neg(mut self) -> Self {
        if !self.value.is_zero() {
            self.negative = !self.negative;
        }
        self
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for Signed<Decimal256> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            value: arbitrary_decimal_256(u)?,
            negative: u.arbitrary()?,
        })
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for Signed<Collateral> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            value: u.arbitrary()?,
            negative: u.arbitrary()?,
        })
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for Signed<Notional> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            value: u.arbitrary()?,
            negative: u.arbitrary()?,
        })
    }
}

/// A signed number type with high fidelity.
///
/// Similar in spirit to cosmwasm_bignumber::Decimal256 - it is
/// a more ergonomic wrapper around cosmwasm-std by making more things public
/// but we also add negative values and other methods as-needed
///
/// MANY OF THE METHODS ARE COPY/PASTE FROM cosmwasm_std
/// the hope is that this is a temporary hack until `cosmwasm_math` lands
pub type Number = Signed<Decimal256>;

/// Ensure that the inner value is never 0.
#[derive(PartialEq, Eq, Clone, Copy, PartialOrd, Ord, Debug)]
pub struct NonZero<T>(T);

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for NonZero<Decimal256> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let v = arbitrary_decimal_256(u)?;
        if v.is_zero() {
            Ok(Self(Decimal256::one()))
        } else {
            Ok(Self(v))
        }
    }
}
#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for NonZero<LpToken> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        NonZero::<Decimal256>::arbitrary(u).map(|v| Self(LpToken::from_decimal256(v.0)))
    }
}
#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for NonZero<Collateral> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        NonZero::<Decimal256>::arbitrary(u).map(|v| Self(Collateral::from_decimal256(v.0)))
    }
}

/// A special case of [NonZero] which stores a big endian array of data.
///
/// Purpose: this is intended to be used as a key in a cw-storage-plus `Map`. This wouldn't be necessary if cw-storage-plus allowed non-reference

/// A [Number] which is always greater than zero.
///
/// This is useful for representing things like price.
pub type NumberGtZero = NonZero<Decimal256>;

impl<T: UnsignedDecimal> NonZero<T> {
    /// Convert into a big-endian array.
    pub fn to_be_bytes(&self) -> [u8; 32] {
        self.0.into_decimal256().atomics().to_be_bytes()
    }

    /// Convert raw bytes into this value.
    ///
    /// Intended for use with cw-storage-plus.
    pub fn from_be_bytes(bytes: [u8; 32]) -> Option<Self> {
        if bytes == [0; 32] {
            None
        } else {
            Some(NonZero(T::from_decimal256(Decimal256::new(
                Uint256::from_be_bytes(bytes),
            ))))
        }
    }

    /// Get the underlying raw value.
    pub fn raw(self) -> T {
        self.0
    }

    /// Turn into a signed value.
    pub fn into_signed(self) -> Signed<T> {
        self.0.into()
    }

    /// Try to convert a raw value into a [NonZero].
    pub fn new(src: T) -> Option<Self> {
        if src.is_zero() {
            None
        } else {
            Some(NonZero(src))
        }
    }

    /// Convert into a general purpose [Decimal256].
    pub fn into_decimal256(self) -> Decimal256 {
        self.0.into_decimal256()
    }

    /// Convert into `NonZero<Decimal>`
    pub fn into_number_gt_zero(self) -> NumberGtZero {
        NonZero::<Decimal256>(self.into_decimal256())
    }

    /// Convert into a general purpose [Number].
    pub fn into_number(self) -> Signed<Decimal256> {
        self.0.into_number()
    }

    /// Try to convert a general purpose [Number] into this type.
    pub fn try_from_number(src: Signed<Decimal256>) -> Option<Self> {
        T::try_from_number(src).ok().and_then(NonZero::new)
    }

    /// Try to convert a general purpose [Decimal256] into this type.
    pub fn try_from_decimal(src: Decimal256) -> Option<Self> {
        NonZero::new(T::from_decimal256(src))
    }

    /// Try to convert a signed value into a non-zero.
    pub fn try_from_signed(src: Signed<T>) -> Result<Self> {
        src.try_into_non_negative_value()
            .and_then(NonZero::new)
            .with_context(|| format!("Could not converted signed value {src} into NonZero"))
    }

    /// Add an unsigned value to this non-zero
    ///
    /// This can fail due to overflow error, but is guaranteed to not give a
    /// value of 0.
    pub fn checked_add(self, rhs: T) -> Result<Self> {
        self.raw()
            .checked_add(rhs)
            .context("NonZero::checked_add overflow")
            .map(|x| NonZero::new(x).expect("Impossible! NonZero::checked_add returned 0"))
    }

    /// Subtract an unsigned value.
    ///
    /// This can fail if the result would be either negative or zero.
    pub fn checked_sub(self, rhs: T) -> Result<Self> {
        self.raw()
            .checked_sub(rhs)
            .ok()
            .and_then(NonZero::new)
            .with_context(|| format!("NonZero::checked_sub: cannot perform {self} - {rhs}"))
    }

    /// Try to add a signed value to this, erroring if it results in a negative
    /// or zero result.
    pub fn checked_add_signed(self, rhs: Signed<T>) -> Result<Self> {
        NonZero::try_from_signed(self.into_signed().checked_add(rhs)?)
            .with_context(|| format!("{self} + {rhs}"))
    }

    /// Try to subtract a signed value from this, erroring if it results in a
    /// negative or zero result.
    pub fn checked_sub_signed(self, rhs: Signed<T>) -> Result<Self> {
        NonZero::try_from_signed(self.into_signed().checked_add(-rhs)?)
            .with_context(|| format!("{self} - {rhs}"))
    }

    /// The value 1.
    pub fn one() -> Self {
        Self(T::from_decimal256(Decimal256::one()))
    }
}

impl<T: UnsignedDecimal> From<NonZero<T>> for Signed<T> {
    fn from(src: NonZero<T>) -> Self {
        Signed::new_positive(src.0)
    }
}

impl<T: UnsignedDecimal> TryFrom<Signed<T>> for NonZero<T> {
    type Error = anyhow::Error;

    fn try_from(value: Signed<T>) -> Result<Self, Self::Error> {
        if value.is_strictly_positive() {
            Ok(NonZero(value.value()))
        } else {
            Err(anyhow::anyhow!(
                "Cannot convert Signed to NonZero, value is {value}"
            ))
        }
    }
}

impl Collateral {
    /// Multiply by the given [Decimal256]
    pub fn checked_mul_dec(self, rhs: Decimal256) -> Result<Collateral> {
        self.0
            .checked_mul(rhs)
            .map(Collateral)
            .with_context(|| format!("Collateral::checked_mul_dec failed on {self} * {rhs}"))
    }

    /// Divide by the given [Decimal256]
    pub fn checked_div_dec(self, rhs: Decimal256) -> Result<Collateral> {
        self.0
            .checked_div(rhs)
            .map(Collateral)
            .with_context(|| format!("Collateral::checked_div_dec failed on {self} * {rhs}"))
    }

    /// Divide by a non-zero decimal.
    pub fn div_non_zero_dec(self, rhs: NonZero<Decimal256>) -> Collateral {
        Collateral::from_decimal256(self.into_decimal256() / rhs.into_decimal256())
    }

    /// Divide by a non-zero collateral, returning a ratio between the two.
    pub fn div_non_zero(self, rhs: NonZero<Collateral>) -> Decimal256 {
        self.into_decimal256() / rhs.into_decimal256()
    }
}

impl Usd {
    /// Multiply by the given [Decimal256]
    pub fn checked_mul_dec(self, rhs: Decimal256) -> Result<Usd> {
        self.0
            .checked_mul(rhs)
            .map(Usd)
            .with_context(|| format!("Usd::checked_mul_ratio failed on {self} * {rhs}"))
    }
}

impl NonZero<Collateral> {
    /// Multiply by the given non-zero decimal.
    pub fn checked_mul_non_zero(self, rhs: NonZero<Decimal256>) -> Result<NonZero<Collateral>> {
        self.0.checked_mul_dec(rhs.raw()).map(|x| {
            debug_assert!(!x.is_zero());
            NonZero(x)
        })
    }

    /// Divide two non-zero collateral values to get the ratio between them.
    ///
    /// This can be used for cases like calculating the max gains.
    pub fn checked_div_collateral(self, rhs: NonZero<Collateral>) -> Result<NonZero<Decimal256>> {
        Ok(NonZero(
            self.into_decimal256().checked_div(rhs.into_decimal256())?,
        ))
    }
}

impl<T: UnsignedDecimal> Signed<T> {
    /// Multiply by a raw number
    pub fn checked_mul_number(self, rhs: Signed<Decimal256>) -> Result<Self> {
        self.into_number().checked_mul(rhs).map(Self::from_number)
    }
}

/// How much to divide an atomic value by to get to an LP token amount.
/// The token uses 6 digits of precision, and Decimal256 uses 18 digits of precision.
/// So to truncate the Decimal256's atomic representation to the Uint128 representation,
/// we need to remove 12 digits (18 - 6).
const LP_TOKEN_DIVIDER: u64 = 1_000_000_000_000;

impl LpToken {
    /// The hard-coded precision of the LP and xLP token contracts.
    pub const PRECISION: u8 = 6;

    /// Convert into a u128 representation for contract interactions.
    ///
    /// Note that this is a lossy conversion, and will truncate some data.
    pub fn into_u128(self) -> Result<u128> {
        Ok(Uint128::try_from(
            self.into_decimal256()
                .atomics()
                .checked_div(LP_TOKEN_DIVIDER.into())?,
        )?
        .u128())
    }

    /// Convert from a u128 representation.
    pub fn from_u128(x: u128) -> Result<Self> {
        Ok(LpToken::from_decimal256(Decimal256::from_atomics(
            x,
            Self::PRECISION.into(),
        )?))
    }
}

impl LvnToken {
    /// Multiply by the given [Decimal256]
    pub fn checked_mul_dec(self, rhs: Decimal256) -> Result<LvnToken> {
        self.0
            .checked_mul(rhs)
            .map(LvnToken)
            .with_context(|| format!("LvnToken::checked_mul failed on {self} * {rhs}"))
    }

    /// Divide by the given [Decimal256]
    pub fn checked_div_dec(self, rhs: Decimal256) -> Result<LvnToken> {
        self.0
            .checked_div(rhs)
            .map(LvnToken)
            .with_context(|| format!("LvnToken::checked_div failed on {self} / {rhs}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lp_token_u128_roundtrip() {
        assert_eq!(
            LpToken::from_str("12.3456789")
                .unwrap()
                .into_u128()
                .unwrap(),
            12345678
        );
        assert_eq!(
            LpToken::from_str("12.345678").unwrap(),
            LpToken::from_u128(12345678).unwrap(),
        );
    }

    #[test]
    fn floor_unsigned_type_with_precision() {
        unsigned!(UnsignedStruct);

        assert_eq!(
            UnsignedStruct::from_str("12.3456789")
                .unwrap()
                .floor_with_precision(2),
            UnsignedStruct::from_str("12.34").unwrap()
        );
    }
}
