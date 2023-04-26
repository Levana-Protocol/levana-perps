// Note: we use f64 throughout since exact precision isn't necessary. This is a
// modeling system, _not_ a financial system.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Amount of USD-denominated value.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize)]
pub(crate) struct Usd(pub(crate) f64);

/// Amount of an asset, presumably OSMO.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize)]
pub(crate) struct Asset(pub(crate) f64);

/// [Usd] per [Asset]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize)]
pub(crate) struct Price(pub(crate) f64);

/// K-parameter for an AMM
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub(crate) struct AmmK(pub(crate) f64);

/// A balance containing both USD and the asset
#[derive(Default, Debug)]
pub(crate) struct Wallet {
    pub(crate) usd: Usd,
    pub(crate) asset: Asset,
}

/// Direction of a position
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Long,
    Short,
}

impl Add<Usd> for Wallet {
    type Output = Wallet;

    fn add(mut self, rhs: Usd) -> Self::Output {
        self += rhs;
        self
    }
}

impl AddAssign<Usd> for Wallet {
    fn add_assign(&mut self, rhs: Usd) {
        self.usd.0 += rhs.0;
    }
}

impl Add<Asset> for Wallet {
    type Output = Wallet;

    fn add(mut self, rhs: Asset) -> Self::Output {
        self += rhs;
        self
    }
}

impl AddAssign<Asset> for Wallet {
    fn add_assign(&mut self, rhs: Asset) {
        self.asset.0 += rhs.0;
    }
}

impl Add for Wallet {
    type Output = Wallet;

    fn add(mut self, rhs: Self) -> Self::Output {
        self += rhs;
        self
    }
}

impl AddAssign for Wallet {
    fn add_assign(&mut self, Wallet { usd, asset }: Self) {
        *self += usd;
        *self += asset;
    }
}

impl Div<Asset> for Usd {
    type Output = Price;

    fn div(self, rhs: Asset) -> Self::Output {
        Price(self.0 / rhs.0)
    }
}

impl Div<Price> for Usd {
    type Output = Asset;

    fn div(self, rhs: Price) -> Self::Output {
        Asset(self.0 / rhs.0)
    }
}

impl Mul<Asset> for Usd {
    type Output = AmmK;

    fn mul(self, rhs: Asset) -> Self::Output {
        AmmK(self.0 * rhs.0)
    }
}

impl AddAssign for Asset {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Div<Asset> for AmmK {
    type Output = Usd;

    fn div(self, rhs: Asset) -> Self::Output {
        Usd(self.0 / rhs.0)
    }
}

impl Div<Usd> for AmmK {
    type Output = Asset;

    fn div(self, rhs: Usd) -> Self::Output {
        Asset(self.0 / rhs.0)
    }
}

impl Mul<Price> for Asset {
    type Output = Usd;

    fn mul(self, rhs: Price) -> Self::Output {
        Usd(self.0 * rhs.0)
    }
}

impl Div for Usd {
    type Output = f64;

    fn div(self, rhs: Self) -> Self::Output {
        self.0 / rhs.0
    }
}

impl Mul<f64> for Usd {
    type Output = Usd;

    fn mul(self, rhs: f64) -> Self::Output {
        Usd(self.0 * rhs)
    }
}

impl Add for Usd {
    type Output = Usd;

    fn add(self, rhs: Self) -> Self::Output {
        Usd(self.0 + rhs.0)
    }
}

impl AddAssign for Usd {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Usd {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Neg for Usd {
    type Output = Usd;

    fn neg(self) -> Self::Output {
        Usd(-self.0)
    }
}

impl Mul<f64> for Asset {
    type Output = Asset;

    fn mul(self, rhs: f64) -> Self::Output {
        Asset(self.0 * rhs)
    }
}

impl Sub<Usd> for Usd {
    type Output = Usd;

    fn sub(self, rhs: Usd) -> Self::Output {
        Usd(self.0 - rhs.0)
    }
}

impl Add<Asset> for Asset {
    type Output = Asset;

    fn add(self, rhs: Asset) -> Self::Output {
        Asset(self.0 + rhs.0)
    }
}

impl Div<Asset> for Asset {
    type Output = f64;

    fn div(self, rhs: Asset) -> Self::Output {
        self.0 / rhs.0
    }
}

impl Asset {
    pub(crate) fn abs(self) -> Self {
        Asset(-self.0)
    }
}

impl Neg for Asset {
    type Output = Asset;

    fn neg(self) -> Self::Output {
        Asset(-self.0)
    }
}

impl Sub<Asset> for Asset {
    type Output = Asset;

    fn sub(self, rhs: Asset) -> Self::Output {
        Asset(self.0 - rhs.0)
    }
}

impl Div<f64> for Usd {
    type Output = Usd;

    fn div(self, rhs: f64) -> Self::Output {
        Usd(self.0 / rhs)
    }
}
