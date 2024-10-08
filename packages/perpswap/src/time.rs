//! Types to represent timestamps and durations.
use crate::prelude::*;
use crate::{
    error::{ErrorDomain, ErrorId, PerpError},
    perp_error,
};
use anyhow::Result;
#[cfg(feature = "chrono")]
use chrono::{DateTime, TimeZone, Utc};
use cosmwasm_std::{Decimal256, Timestamp as CWTimestamp};
use cw_storage_plus::{KeyDeserialize, Prefixer, PrimaryKey};
use schemars::JsonSchema;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::ops::{Add, Div, Mul, Sub};

/// Essentially a newtype wrapper for [Timestamp] providing additional impls.
///
/// Internal representation in nanoseconds since the epoch. We keep a [u64]
/// directly (instead of a [Timestamp] or [cosmwasm_std::Uint64]) to make it
/// easier to derive some impls. The result is that we need to explicitly
/// implement [Serialize] and [Deserialize] to keep the stringy representation.
#[derive(Debug, Clone, Default, Copy, Eq, PartialEq, Ord, PartialOrd, JsonSchema, Hash)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Timestamp(#[schemars(with = "String")] u64);

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let whole = self.0 / 1_000_000_000;
        let fractional = self.0 % 1_000_000_000;
        write!(f, "{}.{:09}", whole, fractional)
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(NanoVisitor)
    }
}

struct NanoVisitor;

impl<'de> Visitor<'de> for NanoVisitor {
    type Value = Timestamp;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("nanoseconds since epoch, string-encoded")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v.parse::<u64>() {
            Ok(v) => Ok(Timestamp(v)),
            Err(e) => Err(E::custom(format!("invalid Nano '{v}' - {e}"))),
        }
    }
}

impl Timestamp {
    /// Construct a new value from the given number of nanoseconds since the
    /// epoch
    pub fn from_nanos(nanos: u64) -> Self {
        Timestamp(nanos)
    }

    /// Construct a new value from the given number of seconds since the
    /// epoch
    pub fn from_seconds(seconds: u64) -> Self {
        Timestamp(seconds * 1_000_000_000)
    }

    /// Construct a new value from the given number of millisecond since the epoch.
    pub fn from_millis(millis: u64) -> Self {
        Timestamp(millis * 1_000_000)
    }

    /// Add the given number of seconds to the given timestamp
    pub fn plus_seconds(self, secs: u64) -> Self {
        self + Duration::from_seconds(secs)
    }

    /// Subtract two timestamps to get the duration between them.
    ///
    /// Will fail if the right hand side is greater than the left hand side.
    pub fn checked_sub(self, rhs: Self, desc: &str) -> Result<Duration> {
        #[derive(serde::Serialize)]
        struct Data {
            lhs: Timestamp,
            rhs: Timestamp,
            desc: String,
        }
        match self.0.checked_sub(rhs.0) {
            Some(x) => Ok(Duration(x)),
            None => Err(perp_anyhow_data!(
                ErrorId::TimestampSubtractUnderflow,
                ErrorDomain::Default,
                Data {
                    lhs: self,
                    rhs,
                    desc: desc.to_owned()
                },
                "Invalid timestamp subtraction during. Action: {desc}. Values: {self} - {rhs}"
            )),
        }
    }

    #[cfg(feature = "chrono")]
    /// Convert into a chrono DateTime<Utc>
    pub fn try_into_chrono_datetime(self) -> Result<DateTime<Utc>> {
        let secs = self.0 / 1_000_000_000;
        let nanos = self.0 % 1_000_000_000;

        Utc.timestamp_opt(secs.try_into()?, nanos.try_into()?)
            .single()
            .with_context(|| format!("Could not convert {self} into DateTime<Utc>"))
    }
}

// Lossless conversions. In the future, we may want to focus on using this type
// throughout the codebase instead of Timestamp, in which case removing these
// impls and instead having explicit helper functions may help identify stray
// conversions still occurring.
impl From<Timestamp> for CWTimestamp {
    fn from(Timestamp(nanos): Timestamp) -> Self {
        CWTimestamp::from_nanos(nanos)
    }
}

impl From<CWTimestamp> for Timestamp {
    fn from(timestamp: CWTimestamp) -> Self {
        Timestamp(timestamp.nanos())
    }
}

impl<'a> PrimaryKey<'a> for Timestamp {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Timestamp;
    type SuperSuffix = Timestamp;

    fn key(&self) -> Vec<cw_storage_plus::Key> {
        self.0.key()
    }
}

impl KeyDeserialize for Timestamp {
    type Output = Timestamp;

    const KEY_ELEMS: u16 = 1;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        u64::from_vec(value).map(Timestamp)
    }
}

impl<'a> Prefixer<'a> for Timestamp {
    fn prefix(&self) -> Vec<cw_storage_plus::Key> {
        self.0.prefix()
    }
}

/// A duration of time measured in nanoseconds
#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
pub struct Duration(u64);

impl Duration {
    /// Construct a [Duration] from a given number of nanoseconds.
    pub(crate) fn from_nanos(nanos: u64) -> Self {
        Duration(nanos)
    }

    /// Returns the underlying nanos value as a u64
    pub fn as_nanos(&self) -> u64 {
        self.0
    }

    /// Convert to milliseconds and represent as a [Number].
    ///
    /// This is intended for performing calculations. Remember that this is a lossy conversion!
    pub fn as_ms_number_lossy(&self) -> Number {
        Number::from(self.0 / 1_000_000)
    }

    /// Convert to milliseconds and represent as a [Decimal256].
    ///
    /// This is intended for performing calculations. Remember that this is a lossy conversion!
    pub fn as_ms_decimal_lossy(&self) -> Decimal256 {
        Decimal256::from_atomics(self.0, 6)
            .expect("as_ms_decimal_lossy failed, but range won't allow that to happen")
    }

    /// Convert a number of seconds into a [Duration].
    pub const fn from_seconds(seconds: u64) -> Self {
        Duration(seconds * 1_000_000_000)
    }
}

// Arithmetic operators. Consider removing these impls in favor of checked
// versions in the future to avoid panicking. Leaving for now to ease
// conversion.

impl Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Duration) -> Self::Output {
        Timestamp(self.0 + rhs.0)
    }
}

impl Sub<Duration> for Duration {
    type Output = Duration;

    fn sub(self, rhs: Duration) -> Self::Output {
        Duration(self.0 - rhs.0)
    }
}

impl Add<Duration> for Duration {
    type Output = Duration;

    fn add(self, rhs: Duration) -> Self::Output {
        Duration(self.0 + rhs.0)
    }
}

impl Mul<u64> for Duration {
    type Output = Duration;

    fn mul(self, rhs: u64) -> Self::Output {
        Duration(self.0 * rhs)
    }
}

impl Div<u64> for Duration {
    type Output = Duration;

    fn div(self, rhs: u64) -> Self::Output {
        Duration(self.0 / rhs)
    }
}

// Used for questions like "how many epochs do I need to fill up the given duration?"
impl Div<Duration> for Duration {
    type Output = u64;

    fn div(self, rhs: Self) -> Self::Output {
        self.0 / rhs.0
    }
}

impl FromStr for Timestamp {
    type Err = PerpError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let err = |msg: &str| -> PerpError {
            perp_error!(
                ErrorId::Conversion,
                ErrorDomain::Default,
                "error converting {} to Timestamp, {}",
                s,
                msg
            )
        };

        let (seconds, nanos) = s
            .split_once('.')
            .ok_or_else(|| err("missing decimal point"))?;
        let seconds = seconds.parse().map_err(|_| err("unable to parse second"))?;
        let nanos = nanos.parse().map_err(|_| err("unable to parse nanos"))?;

        let timestamp = Timestamp::from_seconds(seconds) + Duration::from_nanos(nanos);

        Ok(timestamp)
    }
}
