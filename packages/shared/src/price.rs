//! Data types and conversion functions for different price representations.
use std::{fmt::Display, str::FromStr};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal256, StdError, StdResult};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use schemars::JsonSchema;

use crate::prelude::*;

/// All prices in the protocol for a given point in time.
///
/// This includes extra information necessary for performing all conversions,
/// such as the [MarketType].
#[cw_serde]
#[derive(Copy, Eq)]
pub struct PricePoint {
    /// Price as used internally by the protocol, in terms of collateral and notional.
    ///
    /// This is generally less useful for external consumers, where
    /// [PricePoint::price_usd] and [PricePoint::price_base] are used.
    pub price_notional: Price,
    /// Price of the collateral asset in terms of USD.
    ///
    /// This is generally used for reporting of values like PnL and trade
    /// volume.
    pub price_usd: PriceCollateralInUsd,
    /// Price of the base asset in terms of the quote.
    pub price_base: PriceBaseInQuote,
    /// Timestamp of when this price was entered into the system.
    pub timestamp: Timestamp,
    /// Is the notional asset USD?
    ///
    /// Used for avoiding lossy conversions to USD when they aren't needed.
    ///
    /// We do not need to track if the collateral asset is USD, since USD can
    /// never be used as collateral directly. Instead, stablecoins would be
    /// used, in which case an explicit price to USD is always needed.
    pub is_notional_usd: bool,
    /// Indicates if this market uses collateral as base or quote, needed for
    /// price conversions.
    pub market_type: MarketType,
}

impl PricePoint {
    /// Convert a base value into collateral.
    pub fn base_to_collateral(&self, base: Base) -> Collateral {
        self.price_notional
            .base_to_collateral(self.market_type, base)
    }

    /// Convert a base value into USD.
    pub fn base_to_usd(&self, base: Base) -> Usd {
        self.price_usd
            .collateral_to_usd(self.base_to_collateral(base))
    }

    /// Convert a non-zero collateral value into base.
    pub fn collateral_to_base_non_zero(&self, collateral: NonZero<Collateral>) -> NonZero<Base> {
        self.price_notional
            .collateral_to_base_non_zero(self.market_type, collateral)
    }

    /// Convert a collateral value into USD.
    pub fn collateral_to_usd(&self, collateral: Collateral) -> Usd {
        self.price_usd.collateral_to_usd(collateral)
    }

    /// Convert a USD value into collateral.
    pub fn usd_to_collateral(&self, usd: Usd) -> Collateral {
        self.price_usd.usd_to_collateral(usd)
    }

    /// Keeps the invariant of a non-zero value
    pub fn collateral_to_usd_non_zero(&self, collateral: NonZero<Collateral>) -> NonZero<Usd> {
        self.price_usd.collateral_to_usd_non_zero(collateral)
    }

    /// Convert a notional value into USD.
    pub fn notional_to_usd(&self, notional: Notional) -> Usd {
        if self.is_notional_usd {
            Usd::from_decimal256(notional.into_decimal256())
        } else {
            self.collateral_to_usd(self.notional_to_collateral(notional))
        }
    }

    /// Convert an amount in notional into an amount in collateral
    pub fn notional_to_collateral(&self, amount: Notional) -> Collateral {
        self.price_notional.notional_to_collateral(amount)
    }

    /// Convert an amount in collateral into an amount in notional
    pub fn collateral_to_notional(&self, amount: Collateral) -> Notional {
        self.price_notional.collateral_to_notional(amount)
    }

    /// Convert a non-zero amount in collateral into a non-zero amount in notional
    pub fn collateral_to_notional_non_zero(
        &self,
        amount: NonZero<Collateral>,
    ) -> NonZero<Notional> {
        NonZero::new(self.collateral_to_notional(amount.raw()))
            .expect("collateral_to_notional_non_zero: impossible 0 result")
    }
}

/// The price of the currency pair, given as `quote / base`, e.g. "20,000 USD per BTC".
#[cw_serde]
#[derive(Copy, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PriceBaseInQuote(NumberGtZero);

impl Display for PriceBaseInQuote {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PriceBaseInQuote {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(PriceBaseInQuote)
    }
}

impl TryFrom<&str> for PriceBaseInQuote {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        PriceBaseInQuote::from_str(value)
    }
}

impl PriceBaseInQuote {
    /// Convert to the internal price representation used by our system, as `collateral / notional`.
    pub fn into_notional_price(self, market_type: MarketType) -> Price {
        Price(match market_type {
            MarketType::CollateralIsQuote => self.0,
            MarketType::CollateralIsBase => self.0.inverse(),
        })
    }

    /// Convert into a [PriceKey] representation.
    pub fn into_price_key(self, market_type: MarketType) -> PriceKey {
        self.into_notional_price(market_type).into()
    }

    /// Try to convert a signed decimal into a price.
    pub fn try_from_number(raw: Number) -> Result<Self> {
        raw.try_into().map(PriceBaseInQuote)
    }

    /// Convert into a signed decimal.
    pub fn into_number(&self) -> Number {
        self.0.into()
    }

    /// Convert into a non-zero decimal.
    pub fn into_non_zero(&self) -> NonZero<Decimal256> {
        self.0
    }
}

/// PriceBaseInQuote converted to USD
#[cw_serde]
#[derive(Copy, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PriceCollateralInUsd(NumberGtZero);

impl Display for PriceCollateralInUsd {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PriceCollateralInUsd {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(PriceCollateralInUsd)
    }
}

impl PriceCollateralInUsd {
    /// Try to convert from a signed decimal.
    pub fn try_from_number(raw: Number) -> Result<Self> {
        raw.try_into().map(PriceCollateralInUsd)
    }

    /// Convert from a non-zero decimal.
    pub fn from_non_zero(raw: NonZero<Decimal256>) -> Self {
        PriceCollateralInUsd(raw)
    }

    /// The price point of 1
    pub fn one() -> Self {
        Self(NonZero::one())
    }

    /// Convert into a signed decimal
    pub fn into_number(&self) -> Number {
        self.0.into()
    }

    /// Convert a collateral value into USD.
    fn collateral_to_usd(&self, collateral: Collateral) -> Usd {
        Usd::from_decimal256(collateral.into_decimal256() * self.0.raw())
    }

    /// Convert a USD value into collateral.
    fn usd_to_collateral(&self, usd: Usd) -> Collateral {
        Collateral::from_decimal256(usd.into_decimal256() / self.0.raw())
    }

    /// Keeps the invariant of a non-zero value
    fn collateral_to_usd_non_zero(&self, collateral: NonZero<Collateral>) -> NonZero<Usd> {
        NonZero::new(Usd::from_decimal256(
            collateral.into_decimal256() * self.0.raw(),
        ))
        .expect("collateral_to_usd_non_zero: Impossible! Output cannot be 0")
    }
}

/// The price of the pair as used internally by the protocol, given as `collateral / notional`.
#[derive(
    Debug,
    Copy,
    PartialOrd,
    Ord,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    // It would be better not to have this impl to ensure we never send protocol
    // prices over the wire, but that will break other parts of the API. May want to
    // come back to that later.
    JsonSchema,
)]
pub struct Price(NumberGtZero);

impl Price {
    /// Convert to the external representation.
    pub fn into_base_price(self, market_type: MarketType) -> PriceBaseInQuote {
        PriceBaseInQuote(match market_type {
            MarketType::CollateralIsQuote => self.0,
            MarketType::CollateralIsBase => self.0.inverse(),
        })
    }

    /// Convert a non-zero amount in collateral into an amount in base
    fn collateral_to_base_non_zero(
        &self,
        market_type: MarketType,
        collateral: NonZero<Collateral>,
    ) -> NonZero<Base> {
        NonZero::new(Base::from_decimal256(match market_type {
            MarketType::CollateralIsQuote => collateral.into_decimal256() / self.0.raw(),
            MarketType::CollateralIsBase => collateral.into_decimal256(),
        }))
        .expect("collateral_to_base_non_zero: impossible 0 value as a result")
    }

    /// Convert an amount in base into an amount in collateral
    fn base_to_collateral(&self, market_type: MarketType, amount: Base) -> Collateral {
        Collateral::from_decimal256(match market_type {
            MarketType::CollateralIsQuote => amount.into_decimal256() * self.0.raw(),
            MarketType::CollateralIsBase => amount.into_decimal256(),
        })
    }

    /// Convert an amount in notional into an amount in collateral
    fn notional_to_collateral(&self, amount: Notional) -> Collateral {
        Collateral::from_decimal256(amount.into_decimal256() * self.0.raw())
    }

    /// Convert an amount in collateral into an amount in notional, but with types
    fn collateral_to_notional(&self, amount: Collateral) -> Notional {
        Notional::from_decimal256(amount.into_decimal256() / self.0.raw())
    }

    /// Convert an amount in collateral into an amount in notional, but with types
    pub fn collateral_to_notional_non_zero(
        &self,
        amount: NonZero<Collateral>,
    ) -> NonZero<Notional> {
        NonZero::new(Notional::from_decimal256(
            amount.into_decimal256() / self.0.raw(),
        ))
        .expect("collateral_to_notional_non_zero resulted in 0")
    }
}

impl Display for Price {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.into_number().fmt(f)
    }
}

impl Price {
    /// Attempt to convert a [Number] into a price.
    ///
    /// This will fail on zero or negative numbers. Callers need to ensure that
    /// the incoming [Number] is a protocol price, not a [PriceBaseInQuote].
    pub fn try_from_number(number: Number) -> Result<Price> {
        number
            .try_into()
            .map(Price)
            .context("Cannot convert number to Price")
    }

    /// Convert to a raw [Number].
    ///
    /// Note that in the future we may want to hide this functionality to force
    /// usage of well-typed interfaces here.
    pub fn into_number(&self) -> Number {
        self.0.into()
    }

    /// Convert to a raw [Decimal256].
    pub fn into_decimal256(self) -> Decimal256 {
        self.0.raw()
    }
}

/// A modified version of a [Price] used as a key in a `Map`.
///
/// Due to how cw-storage-plus works, we need to have a reference to a slice,
/// which we can't get from a `Decimal256`. Instead, we store an array directly
/// here and provide conversion functions.
#[derive(Clone)]
pub struct PriceKey([u8; 32]);

impl<'a> PrimaryKey<'a> for PriceKey {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(&self.0)]
    }
}

impl<'a> Prefixer<'a> for PriceKey {
    fn prefix(&self) -> Vec<Key> {
        self.key()
    }
}

impl KeyDeserialize for PriceKey {
    type Output = Price;

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        value
            .try_into()
            .ok()
            .and_then(|bytes| NumberGtZero::from_be_bytes(bytes).map(Price))
            .ok_or_else(|| StdError::generic_err("unable to convert value into Price"))
    }
}

impl From<Price> for PriceKey {
    fn from(price: Price) -> Self {
        PriceKey(price.0.to_be_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_parse() {
        PriceBaseInQuote::from_str("1").unwrap();
        PriceBaseInQuote::from_str("1.0").unwrap();
        PriceBaseInQuote::from_str("1..0").unwrap_err();
        PriceBaseInQuote::from_str(".1").unwrap_err();
        PriceBaseInQuote::from_str("0.1").unwrap();
        PriceBaseInQuote::from_str("-0.1").unwrap_err();
        PriceBaseInQuote::from_str("-0.0").unwrap_err();
        PriceBaseInQuote::from_str("-0").unwrap_err();
        PriceBaseInQuote::from_str("0").unwrap_err();
        PriceBaseInQuote::from_str("0.0").unwrap_err();
        PriceBaseInQuote::from_str("0.001").unwrap();
        PriceBaseInQuote::from_str("0.00100").unwrap();
    }

    #[test]
    fn deserialize_price() {
        let go = serde_json::from_str::<PriceBaseInQuote>;

        go("\"1.0\"").unwrap();
        go("\"1.1\"").unwrap();
        go("\"-1.1\"").unwrap_err();
        go("\"-0\"").unwrap_err();
        go("\"0\"").unwrap_err();
        go("\"0.1\"").unwrap();
    }
}
