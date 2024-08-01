//! Data types to represent leverage
//!
//! Within the perps platform, we have a few different varieties of leverage:
//!
//! * Does the leverage value include direction? Directioned leverage uses
//! negative values to represent shorts and positive to represent longs.
//! Undirectioned is the absolute leverage amount. We use the term "signed"
//! represent leverage types that include the direction.
//!
//! * Notional or base: leverage is given in terms of exposure to the base
//! asset. Within the protocol, for collateral-is-quote markets, the same
//! applies, since base and notional are the same asset. However, for
//! collateral-is-base, we have to convert the leverage in two ways: (1) flip
//! the direction from long to short or short to long, and (2) apply the
//! off-by-one factor to account for the exposure the trader experiences by
//! using the base asset as collateral.
//!
//! We end up with three different data types:
//!
//! * [LeverageToBase] is the the absolute leverage (without direction) from
//! the trader point of view in terms of exposure to the base asset.
//!
//! * [SignedLeverageToBase] is the trader perspective of leverage, but uses
//! negative values to represent shorts.
//!
//! * [SignedLeverageToNotional] is the protocol's perspective of leverage
//! including the sign.
//!
//! It's not necessary to provide a `LeverageToNotional`, since within the
//! protocol we always use signed values. The unsigned version is only for
//! trader/API convenience.
//!
//! To provide a worked example: suppose a trader wants to open a 5x leveraged
//! short. If the market is collateral-is-quote, the [LeverageToBase] value
//! would be `5`, [SignedLeverageToBase] would be `-5`, and
//! [SignedLeverageToNotional] would also be `-5`.
//!
//! By contrast, if the market is collateral-is-base, the external values would
//! remain the same, but [SignedLeverageToNotional] would be `6` from the formula
//! `to_notional = 1 - to_base`.

use crate::prelude::*;

/// The absolute leverage for a position, in terms of the base asset.
///
/// Note that while leverage specified by the trader must be strictly positive
/// (greater than 0), this type allows zero leverage to occur, since calculated
/// leverage within the system based on the off-by-one exposure calculation may
/// end up as 0.
#[cw_serde]
#[derive(Copy, Eq, PartialOrd, Ord)]
pub struct LeverageToBase(Decimal256);

impl LeverageToBase {
    /// Get the raw underlying leverage value.
    pub fn raw(self) -> Decimal256 {
        self.0
    }

    /// Convert to an unsigned decimal.
    pub fn into_decimal256(self) -> Decimal256 {
        self.0
    }

    /// Convert to a signed decimal.
    pub fn into_number(self) -> Signed<Decimal256> {
        self.0.into_signed()
    }

    /// Convert into a [SignedLeverageToBase]
    pub fn into_signed(self, direction: DirectionToBase) -> SignedLeverageToBase {
        match direction {
            DirectionToBase::Long => SignedLeverageToBase(self.0.into_signed()),
            DirectionToBase::Short => SignedLeverageToBase(-self.0.into_signed()),
        }
    }
}

impl From<NonZero<Decimal256>> for LeverageToBase {
    fn from(value: NonZero<Decimal256>) -> Self {
        LeverageToBase(value.raw())
    }
}

impl TryFrom<&str> for LeverageToBase {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl FromStr for LeverageToBase {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse()
            .map(LeverageToBase)
            .context("Invalid LeverageToBase")
    }
}

impl Display for LeverageToBase {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for LeverageToBase {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self(crate::number::arbitrary_decimal_256(u)?))
    }
}

/// The user-specified leverage for a position, with direction expressed as the signed value
///
/// Leverage is always specified by the user in terms of the base currency. In a
/// collateral-is-quote market, that directly becomes the exposure to notional.
/// In a collateral-is-base market, we need to convert that exposure from
/// collateral to notional for internal calculations.
#[cw_serde]
#[derive(Copy)]
pub struct SignedLeverageToBase(Number);

impl SignedLeverageToBase {
    /// Get the leverage in terms of the notional currency.
    ///
    /// If the [MarketType] is [MarketType::CollateralIsQuote], the value is
    /// already in terms of notional, and no change is needed. Otherwise, in a
    /// [MarketType::CollateralIsBase], we have to convert from leverage in
    /// terms of base/collateral into a notional value.
    ///
    /// The formula for converting is `leverage_to_notional = 1 -
    /// leverage_to_base`. The motivation for that is:
    ///
    /// 1. Going long on notional is equivalent to going short on collateral and
    /// vice-versa, therefore we have a negative sign.
    ///
    /// 2. By holding the collateral asset, the trader already has exposure to
    /// its price fluctuation, so we need to represent that by adding 1.
    pub fn into_notional(self, market_type: MarketType) -> Result<SignedLeverageToNotional> {
        Ok(SignedLeverageToNotional(match market_type {
            MarketType::CollateralIsQuote => self.0,
            MarketType::CollateralIsBase => (Number::ONE - self.0)?,
        }))
    }

    /// Split up this value into the direction and absolute leverage.
    pub fn split(self) -> (DirectionToBase, LeverageToBase) {
        let (direction, leverage) = match self.0.try_into_non_negative_value() {
            Some(x) => (DirectionToBase::Long, x),
            None => (DirectionToBase::Short, self.0.abs_unsigned()),
        };
        (direction, LeverageToBase(leverage))
    }

    /// Multiply by active collateral of a position expressed in base
    ///
    /// This returns the position size from a trader perspective, aka the exposure to the base asset.
    pub fn checked_mul_base(self, base: NonZero<Base>) -> Result<Signed<Base>> {
        base.into_signed().checked_mul_number(self.0)
    }
}

impl Display for SignedLeverageToBase {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for SignedLeverageToBase {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(SignedLeverageToBase)
    }
}

/// Leverage calculated based on the protocol's internal representation.
///
/// This is calculated by comparing the notional size of a position against some
/// amount of collateral (either active collateral from the trader or counter
/// collateral from the liquidity pool). One of these values needs to be
/// converted using a [Price], so the leverage will change
/// over time based on exchange rate.
#[derive(Clone, Copy, Debug)]
pub struct SignedLeverageToNotional(Signed<Decimal256>);

impl From<Signed<Decimal256>> for SignedLeverageToNotional {
    fn from(value: Signed<Decimal256>) -> Self {
        SignedLeverageToNotional(value)
    }
}

impl SignedLeverageToNotional {
    /// Extract the direction value
    pub fn direction(self) -> DirectionToNotional {
        match self.0.try_into_non_negative_value() {
            Some(_) => DirectionToNotional::Long,
            None => DirectionToNotional::Short,
        }
    }

    /// Calculate based on notional size, a price point, and some amount of collateral.
    ///
    /// Can fail because of overflow issues, but is otherwise guaranteed to
    /// return a sensible value since the collateral is a non-zero value.
    pub fn calculate(
        notional_size: Signed<Notional>,
        price_point: &PricePoint,
        collateral: NonZero<Collateral>,
    ) -> Self {
        let notional_size_in_collateral =
            notional_size.map(|x| price_point.notional_to_collateral(x));
        SignedLeverageToNotional(notional_size_in_collateral.map(|x| x.div_non_zero(collateral)))
    }

    /// Convert into the raw value.
    pub fn into_number(self) -> Signed<Decimal256> {
        self.0
    }

    /// Convert into an [SignedLeverageToBase].
    pub fn into_base(self, market_type: MarketType) -> Result<SignedLeverageToBase> {
        Ok(SignedLeverageToBase(match market_type {
            MarketType::CollateralIsQuote => self.0,
            MarketType::CollateralIsBase => (Number::ONE - self.0)?,
        }))
    }

    /// Multiply by active collateral of a position, returning the notional size in collateral of a position.
    pub fn checked_mul_collateral(
        self,
        collateral: NonZero<Collateral>,
    ) -> Result<Signed<Collateral>> {
        collateral.into_signed().checked_mul_number(self.0)
    }
}
