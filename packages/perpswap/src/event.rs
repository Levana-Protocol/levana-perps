//! Helpers for parsing event data into well typed event data types.
use std::str::FromStr;

use crate::direction::DirectionToBase;
use crate::leverage::LeverageToBase;
use crate::prelude::*;
use anyhow::Context;
use cosmwasm_std::Event;
use serde::de::DeserializeOwned;

use crate::error::{ErrorDomain, ErrorId};
use crate::perp_anyhow;

/// Extension trait to add methods to native cosmwasm events
pub trait CosmwasmEventExt {
    // these are the only two that require implementation
    // everything else builds on these

    /// Does the event have the given attribute?
    fn has_attr(&self, key: &str) -> bool;

    /// Parse the value associated with the key, if it exists
    fn try_map_attr<B>(&self, key: &str, f: impl Fn(&str) -> B) -> Option<B>;

    /// Parse the value associated with the key as JSON, if it exists
    fn try_json_attr<B: DeserializeOwned>(&self, key: &str) -> anyhow::Result<Option<B>> {
        match self.try_map_attr(key, |s| serde_json::from_str(s)) {
            None => Ok(None),
            Some(x) => Ok(Some(x?)),
        }
    }

    /// Parse the value associated with the key as JSON
    fn json_attr<B: DeserializeOwned>(&self, key: &str) -> anyhow::Result<B> {
        self.map_attr_result(key, |s| {
            serde_json::from_str(s).map_err(anyhow::Error::from)
        })
    }

    /// Parse the value associated with the key as a u64
    fn u64_attr(&self, key: &str) -> anyhow::Result<u64> {
        self.map_attr_result(key, |s| s.parse().map_err(anyhow::Error::from))
    }

    /// Parse the value associated with the key as a u64, if it exists
    fn try_u64_attr(&self, key: &str) -> anyhow::Result<Option<u64>> {
        match self.try_map_attr(key, |s| s.parse()) {
            None => Ok(None),
            Some(x) => Ok(Some(x?)),
        }
    }

    /// Parse a timestamp attribute
    fn timestamp_attr(&self, key: &str) -> anyhow::Result<Timestamp> {
        self.map_attr_result(key, Timestamp::from_str)
    }

    /// Parse a timestamp attribute, if it exists
    fn try_timestamp_attr(&self, key: &str) -> anyhow::Result<Option<Timestamp>> {
        self.try_map_attr(key, Timestamp::from_str).transpose()
    }

    /// Parse an unsigned decimal attribute
    fn decimal_attr<T: UnsignedDecimal>(&self, key: &str) -> anyhow::Result<T> {
        self.map_attr_result(key, |s| {
            s.parse()
                .ok()
                .with_context(|| format!("decimal_attr failed on key {key} and value {s}"))
        })
    }

    /// Parse a non-zero (strictly positive) decimal attribute
    fn non_zero_attr<T: UnsignedDecimal>(&self, key: &str) -> anyhow::Result<NonZero<T>> {
        self.map_attr_result(key, |s| {
            s.parse()
                .ok()
                .with_context(|| format!("non_zero_attr failed on key {key} and value {s}"))
        })
    }

    /// Parse a signed decimal attribute
    fn signed_attr<T: UnsignedDecimal>(&self, key: &str) -> anyhow::Result<Signed<T>> {
        self.map_attr_result(key, |s| {
            s.parse()
                .ok()
                .with_context(|| format!("signed_attr failed on key {key} and value {s}"))
        })
    }

    /// Parse a signed decimal attribute
    fn number_attr<T: UnsignedDecimal>(&self, key: &str) -> anyhow::Result<Signed<T>> {
        self.map_attr_result(key, |s| s.parse())
    }

    /// Parse an optional signed decimal attribute
    fn try_number_attr<T: UnsignedDecimal>(&self, key: &str) -> anyhow::Result<Option<Signed<T>>> {
        self.try_map_attr(key, |s| {
            s.parse()
                .ok()
                .with_context(|| format!("try_number_attr failed parse on key {key} and value {s}"))
        })
        .transpose()
    }

    /// Parse an optional unsigned decimal attribute
    fn try_decimal_attr<T: UnsignedDecimal>(&self, key: &str) -> anyhow::Result<Option<T>> {
        self.try_map_attr(key, |s| {
            s.parse().ok().with_context(|| {
                format!("try_decimal_attr failed parse on key {key} and value {s}")
            })
        })
        .transpose()
    }

    /// Parse an optional price
    fn try_price_base_in_quote(&self, key: &str) -> anyhow::Result<Option<PriceBaseInQuote>> {
        self.try_map_attr(key, |s| {
            s.parse().ok().with_context(|| {
                format!("try_price_base_in_quote failed parse on key {key} and value {s}")
            })
        })
        .transpose()
    }

    /// Parse a string attribute
    fn string_attr(&self, key: &str) -> anyhow::Result<String> {
        self.map_attr_ok(key, |s| s.to_string())
    }

    /// Parse a bool-as-string attribute
    fn bool_attr(&self, key: &str) -> anyhow::Result<bool> {
        self.string_attr(key)
            .and_then(|s| s.parse::<bool>().map_err(|err| err.into()))
    }

    /// Parse an attribute with a position direction (to base)
    fn direction_attr(&self, key: &str) -> anyhow::Result<DirectionToBase> {
        self.map_attr_result(key, |s| match s {
            "long" => Ok(DirectionToBase::Long),
            "short" => Ok(DirectionToBase::Short),
            _ => Err(anyhow::anyhow!("Invalid direction: {s}")),
        })
    }

    /// Parse an attribute with the absolute leverage (to base)
    fn leverage_to_base_attr(&self, key: &str) -> anyhow::Result<LeverageToBase> {
        self.map_attr_result(key, LeverageToBase::from_str)
    }

    /// Parse an optional attribute with the absolute leverage (to base)
    fn try_leverage_to_base_attr(&self, key: &str) -> anyhow::Result<Option<LeverageToBase>> {
        self.try_map_attr(key, LeverageToBase::from_str).transpose()
    }

    /// Parse an address attribute without checking validity
    fn unchecked_addr_attr(&self, key: &str) -> anyhow::Result<Addr> {
        self.map_attr_ok(key, |s| Addr::unchecked(s))
    }

    /// Parse an optional address attribute without checking validity
    fn try_unchecked_addr_attr(&self, key: &str) -> anyhow::Result<Option<Addr>> {
        self.try_map_attr(key, |s| Ok(Addr::unchecked(s)))
            .transpose()
    }

    /// Require an attribute and apply a function to the raw string value
    fn map_attr_ok<B>(&self, key: &str, f: impl Fn(&str) -> B) -> anyhow::Result<B> {
        match self.try_map_attr(key, f) {
            Some(x) => Ok(x),
            None => Err(perp_anyhow!(
                ErrorId::Any,
                ErrorDomain::Default,
                "no such key {}",
                key
            )),
        }
    }

    /// Require an attribute and try to parse its value with the given function
    fn map_attr_result<B>(
        &self,
        key: &str,
        f: impl Fn(&str) -> anyhow::Result<B>,
    ) -> anyhow::Result<B> {
        // just need to remove the one level of nesting for "no such key"
        self.map_attr_ok(key, f)?
    }
}

impl CosmwasmEventExt for Event {
    fn has_attr(&self, key: &str) -> bool {
        self.attributes.iter().any(|a| a.key == key)
    }
    fn try_map_attr<B>(&self, key: &str, f: impl Fn(&str) -> B) -> Option<B> {
        self.attributes.iter().find_map(|a| {
            if a.key == key {
                Some(f(a.value.as_str()))
            } else {
                None
            }
        })
    }
}
