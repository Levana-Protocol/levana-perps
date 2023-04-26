//! Provides a number of data types, methods, and traits to have more
//! fine-grained and strongly-typed control of numeric data.
//!
//! # Base, quote, notional, and collateral
//!
//! In general markets, [a currency
//! pair](https://www.investopedia.com/terms/c/currencypair.asp) like `BTC/USD`
//! consists of a *base currency* (`BTC`) and a *quote currency* (`USD`). In our
//! platform, we talk about the *notional* and *collateral* assets, where the
//! collateral asset is what gets deposited in the platform and notional is
//! (generally) the risk asset being speculated on.
//!
//! Generally speaking, in most perpetual swaps platforms, the base and notional
//! assets are the same, and the quote and collateral assets are the same.
//! However, our platform supports a concept called *crypto denominated pairs*.
//! In these, we use the base/risk asset as collateral and quote is the
//! notional. This causes a cascade of changes around leverage and price
//! management.
//!
//! However, all those changes are _internal to the protocol_. The user-facing
//! API should almost exclusively care about base and quote (besides the fact
//! that the user will interact with the contracts by depositing and withdrawing
//! collateral assets). The purpose of this module is to provide data types that
//! provide safety and clarity around whether we're dealing with the base/quote
//! view of the world (user-facing) or notional/collateral (internal) view.
//!
//! # Decimal256, NonZero, Signed, and UnsignedDecimal
//!
//! Math generally uses [Decimal256](cosmwasm_std::Decimal256).
//! However, this type alone cannot express negative numbers, and we often want
//! to ensure additional constraints at compile time.
//!
//! A combination of traits and newtype wrappers gives us a robust framework:
//!
//! * `UnsignedDecimal`: a _trait_, not a concrete type, which is implemented
//! for `Collateral`, `Notional`, and several other numeric types.
//!
//! * `NonZero<T>`: a newtype wrapper which ensures that the value is not zero.
//! It's generally used for types where `T: UnsignedDecimal`.
//!
//! * `Signed<T>`: a newtype wrapper which allows for positive or negative
//! values. It's also generally used for types where `T: UnsignedDecimal`.
//!
//! Putting it all together, here are some examples. Note that these are merely
//! illustrative. Real-world problems would require a price point to convert
//! between Collateral and Notional:
//!
//! ### UnsignedDecimal
//!
//! `Collateral` implements `UnsignedDecimal`, and so we can add two `Collateral`
//! values together via `.checked_add()`.
//!
//! However, we cannot add a `Collateral` and some other `Decimal256`. Instead
//! we need to call `.into_decimal256()`, do our math with another `Decimal256`,
//! and then convert it to any `T: UnsignedDecimal` via `T::from_decimal256()`.
//!
//! *example*
//!
//! ```
//! use levana_perpswap_cosmos_shared::number::*;
//! use cosmwasm_std::Decimal256;
//! use std::str::FromStr;
//!
//! let lhs:Collateral = "1.23".parse().unwrap();
//! let rhs:Decimal256 = "4.56".parse().unwrap();
//! let decimal_result = lhs.into_decimal256().checked_add(rhs).unwrap();
//! let output:Notional = Notional::from_decimal256(decimal_result);
//! ```
//!
//! ### NonZero
//!
//! `NonZero<Collateral>` allows us to call various `.checked_*` math methods
//! with another `NonZero<Collateral>`.
//!
//! However, if we want to do math with a different underlying type - we do need
//! to drop down to that common type. There's two approaches (both of which
//! return an Option, in case the resulting value is zero):
//!     
//!   1. If the inner NonZero type stays the same (i.e. it's all `Collateral`)
//!     then call `.raw()` to get the inner type, do your math, and then convert
//!     back to the NonZero wrapper via `NonZero::new()`
//!   2. If you need a `Decimal256`, then call `.into_decimal256()` to get the
//!     underlying `Decimal256` type, do your math, and then convert back to
//!     `NonZero<T>` via `NonZero::new(T::from_decimal256(value))`. This is
//!     usually the case when the type of `T` has changed
//!     (i.e. from `Collateral` to `Notional`)
//!
//! *example 1*
//!
//! ```
//! use levana_perpswap_cosmos_shared::number::*;
//! use cosmwasm_std::Decimal256;
//! use std::str::FromStr;
//!  
//! let lhs:NonZero<Collateral> = "1.23".parse().unwrap();
//! let rhs:Collateral = "4.56".parse().unwrap();
//! let collateral_result = lhs.raw().checked_add(rhs).unwrap();
//! let output:NonZero<Collateral> = NonZero::new(collateral_result).unwrap();
//!
//! ```
//!
//! *example 2*
//!
//! ```
//! use levana_perpswap_cosmos_shared::number::*;
//! use cosmwasm_std::Decimal256;
//! use std::str::FromStr;
//!  
//! let lhs:NonZero<Collateral> = "1.23".parse().unwrap();
//! let rhs:Decimal256 = "4.56".parse().unwrap();
//! let decimal_result = lhs.into_decimal256().checked_add(rhs).unwrap();
//! let notional_result = Notional::from_decimal256(decimal_result);
//! let output:NonZero<Notional> = NonZero::new(notional_result).unwrap();
//!
//! ```
//! ### Signed
//!
//! `Signed<Collateral>` also allows us to call various `.checked_*` math methods
//! when the inner type is the same. However, there are some differences when
//! comparing to the `NonZero` methods:
//!
//!   1. To get the underlying `T`, call `.abs_unsigned()` instead of `.raw()`.
//!   The sign is now lost, it's not a pure raw conversion.
//!
//!   2. To get back from the underlying `T`, call `T::into_signed()`
//!
//!   3. There is no direct conversion to `Decimal256`.
//!
//!   4. There are helpers for the ubiquitous use-case of `Signed<Decimal256>`
//!   This is such a common occurance, it has its own type alias: `Number`.
//!
//! *example 1*
//!
//! ```
//! use levana_perpswap_cosmos_shared::number::*;
//! use cosmwasm_std::Decimal256;
//! use std::str::FromStr;
//!
//! let lhs:Signed<Collateral> = "-1.23".parse().unwrap();
//! let rhs:Decimal256 = "4.56".parse().unwrap();
//! let decimal_result = lhs.abs_unsigned().into_decimal256().checked_mul(rhs).unwrap();
//! let notional_result = Notional::from_decimal256(decimal_result);
//! // bring back our negative sign
//! let output:Signed<Notional> = -notional_result.into_signed();
//! ```
//!
//! *example 2*
//! ```
//! use levana_perpswap_cosmos_shared::number::*;
//! use cosmwasm_std::Decimal256;
//! use std::str::FromStr;
//!
//! let lhs:Signed<Collateral> = "-1.23".parse().unwrap();
//! let rhs:Number = "4.56".parse().unwrap();
//! let number_result = lhs.into_number().checked_mul(rhs).unwrap();
//! let output:Signed<Notional> = Signed::<Notional>::from_number(number_result);
//! ```

mod convert;
pub use convert::*;
mod ops;
pub use ops::*;
mod serialize;
pub use ops::*;
use schemars::schema::{InstanceType, Metadata, SchemaObject};
use schemars::JsonSchema;
mod nonzero;
pub use self::types::*;

mod types;

// schemars could not figure out that it is serialized as a string
// so gotta impl it manually
impl<T: UnsignedDecimal> JsonSchema for Signed<T> {
    fn schema_name() -> String {
        "Signed decimal".to_owned()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut obj = SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            ..Default::default()
        };

        let mut meta = match obj.metadata {
            None => Box::new(Metadata::default()),
            Some(m) => m,
        };

        // would be nice to re-use the doc comments above...
        meta.description = Some(
            r#"
            A signed number type with high fidelity.
            Similar in spirit to cosmwasm_bignumber::Decimal256 - it is
            a more ergonomic wrapper around cosmwasm-std by making more things public
            but we also add negative values and other methods as-needed
        "#
            .to_string(),
        );

        obj.metadata = Some(meta);

        obj.into()
    }
}

impl<T: UnsignedDecimal> Signed<T> {
    /// absolute value
    pub fn abs(self) -> Self {
        Self::new_positive(self.value())
    }

    /// Absolute value, but return the `T` underlying type directly
    pub fn abs_unsigned(self) -> T {
        self.value()
    }

    /// Checks if this number is greater than 0.
    pub fn is_strictly_positive(&self) -> bool {
        !self.is_zero() && !self.is_negative()
    }

    /// Checks if this number is greater than or equal to 0.
    pub fn is_positive_or_zero(&self) -> bool {
        !self.is_negative()
    }

    /// Is the value 0?
    pub fn is_zero(&self) -> bool {
        self.value().is_zero()
    }

    /// Apply a function to the inner value and rewrap.
    ///
    /// This will keep the current sign (positive or negative) in place,
    /// respecting invariants that a value of 0 must have negative set to false.
    pub fn map<U: UnsignedDecimal, F: FnOnce(T) -> U>(self, f: F) -> Signed<U> {
        let value = f(self.value());
        if self.is_negative() {
            Signed::new_negative(value)
        } else {
            Signed::new_positive(value)
        }
    }

    /// Like `map` but may fail
    pub fn try_map<E, U: UnsignedDecimal, F: FnOnce(T) -> Result<U, E>>(
        self,
        f: F,
    ) -> Result<Signed<U>, E> {
        f(self.value()).map(|value| {
            if self.is_negative() {
                Signed::new_negative(value)
            } else {
                Signed::new_positive(value)
            }
        })
    }
}

#[cfg(test)]
mod test {
    use super::Number;
    use std::str::FromStr;

    #[test]
    fn number_default() {
        assert_eq!(Number::ZERO, Number::default());
    }

    #[test]
    fn number_serde() {
        let a = Number::from(300u64);
        let b = Number::from(7u64);
        let res = a / b;

        assert_eq!(serde_json::to_value(res).unwrap(), "42.857142857142857142");
        assert_eq!(
            serde_json::from_str::<Number>("\"42.857142857142857142\"").unwrap(),
            res
        );

        let res = -res;

        assert_eq!(serde_json::to_value(res).unwrap(), "-42.857142857142857142");
        assert_eq!(
            serde_json::from_str::<Number>("\"-42.857142857142857142\"").unwrap(),
            res
        );
    }

    #[test]
    fn number_arithmetic() {
        let a = Number::from(300u64);
        let b = Number::from(7u64);

        assert_eq!((a + b).to_string(), "307");
        assert_eq!((a - b).to_string(), "293");
        assert_eq!((b - a).to_string(), "-293");
        assert_eq!((a * b).to_string(), "2100");
        assert_eq!((a / b).to_string(), "42.857142857142857142");

        let a = -a;
        let b = -b;
        assert_eq!((a + b).to_string(), "-307");
        assert_eq!((a - b).to_string(), "-293");
        assert_eq!((b - a).to_string(), "293");
        assert_eq!((a * b).to_string(), "2100");
        assert_eq!((a / b).to_string(), "42.857142857142857142");

        let a = -a;
        assert_eq!((a + b).to_string(), "293");
        assert_eq!((a - b).to_string(), "307");
        assert_eq!((b - a).to_string(), "-307");
        assert_eq!((a * b).to_string(), "-2100");
        assert_eq!((a / b).to_string(), "-42.857142857142857142");
    }

    #[test]
    fn number_cmp() {
        let a = Number::from_str("4.2").unwrap();
        let b = Number::from_str("0.007").unwrap();

        assert!(a > b);
        assert!(a.approx_gt_strict(b));
        assert!(a.approx_gt_relaxed(b));
        assert!(a != b);

        let a = Number::from_str("4.2").unwrap();
        let b = Number::from_str("4.2").unwrap();

        assert!(a <= b);
        assert!(a >= b);
        assert!(a.approx_eq(b));
        assert!(a == b);

        let a = Number::from_str("4.2").unwrap();
        let b = Number::from_str("-4.2").unwrap();

        assert!(a > b);
        assert!(a.approx_gt_strict(b));
        assert!(a.approx_gt_relaxed(b));
        assert!(a != b);

        let a = Number::from_str("-4.2").unwrap();
        let b = Number::from_str("4.2").unwrap();

        assert!(a < b);
        assert!(a.approx_lt_relaxed(b));
        assert!(a != b);

        let a = Number::from_str("-4.5").unwrap();
        let b = Number::from_str("-4.2").unwrap();

        assert!(a < b);
        assert!(a.approx_lt_relaxed(b));
        assert!(a != b);

        let a = Number::from_str("-4.2").unwrap();
        let b = Number::from_str("-4.5").unwrap();

        assert!(a > b);
        assert!(a.approx_gt_strict(b));
        assert!(a.approx_gt_relaxed(b));
        assert!(a != b);
    }

    #[test]
    fn unsigned_key_bytes() {
        let a = Number::from_str("0.9")
            .unwrap()
            .to_unsigned_key_bytes()
            .unwrap();
        let b = Number::from_str("1.0")
            .unwrap()
            .to_unsigned_key_bytes()
            .unwrap();
        let c = Number::from_str("1.9")
            .unwrap()
            .to_unsigned_key_bytes()
            .unwrap();
        let d = Number::from_str("9.0")
            .unwrap()
            .to_unsigned_key_bytes()
            .unwrap();
        let e = Number::from_str("9.1")
            .unwrap()
            .to_unsigned_key_bytes()
            .unwrap();
        assert!(a < b);
        assert!(b < c);
        assert!(c < d);
        assert!(d < e);

        assert!(Number::from_str("-1.0")
            .unwrap()
            .to_unsigned_key_bytes()
            .is_none());
    }

    #[test]
    fn zero_str() {
        let mut a = Number::from_str("0").unwrap();
        a = -a;
        assert_eq!(a.to_string(), "0");

        let a = Number::from_str("-0").unwrap();
        assert_eq!(a.to_string(), "0");
    }

    #[test]
    fn number_u128_with_precision() {
        let _a = Number::from_str("270.15").unwrap();
        let b = Number::from_str("1.000000001").unwrap();
        let c = Number::from(u128::MAX);

        // Typcial use - we will send this number in a BankMsg normally
        assert_eq!(_a.to_u128_with_precision(6).unwrap(), 270_150_000);

        // Demonstrate inherent lossy-ness of doing Number -> u128
        assert_eq!(b.to_u128_with_precision(6).unwrap(), 1_000_000);
        assert_eq!(b.to_u128_with_precision(9).unwrap(), 1_000_000_001);

        // Try 6-decimal precision on a number that would overflow
        assert_eq!(c.to_u128_with_precision(6), None);

        // Try 0-decimal precision on the largest number we can handle
        assert_eq!(c.to_u128_with_precision(0).unwrap(), u128::MAX);
    }

    #[test]
    fn catch_overflow() {
        match Number::MAX.checked_mul(Number::MAX) {
            Ok(_) => {
                panic!("should overflow!");
            }
            Err(e) => {
                if !e.to_string().contains("Overflow") {
                    panic!("wrong error! (got {e})");
                }
            }
        }
    }

    #[test]
    fn basic_multiplication() {
        let num = Number::from_str("1.1").unwrap();
        let twopointtwo = num * 2u64;
        assert_eq!(twopointtwo, Number::from_str("2.2").unwrap());
    }
}
