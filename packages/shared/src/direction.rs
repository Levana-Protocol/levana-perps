//! Different representations of the direction of a position.
//!
//! Positions can either be long or short, but due to the different
//! [MarketType]s supported by perps we need to distinguish between the
//! direction to the base asset versus the notional asset.
use std::array::TryFromSliceError;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{StdError, StdResult};
use cw_storage_plus::{IntKey, Key, KeyDeserialize, Prefixer, PrimaryKey};

use crate::{market_type::MarketType, prelude::*};

/// Direction in terms of notional
#[cw_serde]
#[derive(Eq, Copy)]
#[repr(u8)]
pub enum DirectionToNotional {
    /// Long versus notional
    Long,
    /// Short versus notional
    Short,
}

/// Direction in terms of base
#[cw_serde]
#[derive(Eq, Copy)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum DirectionToBase {
    /// Long versus base
    Long,
    /// Short versus base
    Short,
}

impl DirectionToBase {
    /// Represent as a string, either `long` or `short`
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }

    /// Swap to the opposite direction
    pub fn invert(self) -> Self {
        match self {
            Self::Long => Self::Short,
            Self::Short => Self::Long,
        }
    }

    /// Convert into the direction to notional
    pub fn into_notional(&self, market_type: MarketType) -> DirectionToNotional {
        match (market_type, self) {
            (MarketType::CollateralIsQuote, DirectionToBase::Long) => DirectionToNotional::Long,
            (MarketType::CollateralIsQuote, DirectionToBase::Short) => DirectionToNotional::Short,
            (MarketType::CollateralIsBase, DirectionToBase::Long) => DirectionToNotional::Short,
            (MarketType::CollateralIsBase, DirectionToBase::Short) => DirectionToNotional::Long,
        }
    }
}

impl DirectionToNotional {
    /// Represent as a string, either `long` or `short`
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }

    /// Convert into the direction to base
    pub fn into_base(&self, market_type: MarketType) -> DirectionToBase {
        match (market_type, self) {
            (MarketType::CollateralIsQuote, DirectionToNotional::Long) => DirectionToBase::Long,
            (MarketType::CollateralIsQuote, DirectionToNotional::Short) => DirectionToBase::Short,
            (MarketType::CollateralIsBase, DirectionToNotional::Long) => DirectionToBase::Short,
            (MarketType::CollateralIsBase, DirectionToNotional::Short) => DirectionToBase::Long,
        }
    }

    /// Return positive 1 for long, negative 1 for short
    pub fn sign(&self) -> Number {
        match self {
            DirectionToNotional::Long => Number::ONE,
            DirectionToNotional::Short => Number::NEG_ONE,
        }
    }
}

impl From<&str> for DirectionToNotional {
    fn from(s: &str) -> Self {
        match s {
            "long" => Self::Long,
            "short" => Self::Short,
            _ => unimplemented!(),
        }
    }
}

impl<'a> PrimaryKey<'a> for DirectionToNotional {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        let val: u8 = *self as u8;
        let key = Key::Val8(val.to_cw_bytes());

        vec![key]
    }
}

impl<'a> Prefixer<'a> for DirectionToNotional {
    fn prefix(&self) -> Vec<Key> {
        let val: u8 = *self as u8;
        let key = Key::Val8(val.to_cw_bytes());
        vec![key]
    }
}

impl KeyDeserialize for DirectionToNotional {
    type Output = u8;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        Ok(u8::from_cw_bytes(value.as_slice().try_into().map_err(
            |err: TryFromSliceError| StdError::generic_err(err.to_string()),
        )?))
    }
}
