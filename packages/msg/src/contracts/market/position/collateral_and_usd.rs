//! Defines data types combining collateral and USD.
//!
//! The purpose of this is to avoid accidentally updating one field in a data
//! structure without updating the other.

use crate::prelude::*;

/// Collateral and USD which will always be non-negative.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Copy, Default)]
pub struct CollateralAndUsd {
    collateral: Collateral,
    usd: Usd,
}

impl CollateralAndUsd {
    /// Create a new value from the given collateral
    pub fn new(collateral: Collateral, price_point: &PricePoint) -> Self {
        CollateralAndUsd {
            collateral,
            usd: price_point.collateral_to_usd(collateral),
        }
    }

    /// Creates a new value from the raw collateral and USD values.
    pub fn from_pair(collateral: Collateral, usd: Usd) -> Self {
        CollateralAndUsd { collateral, usd }
    }

    /// Get the collateral component
    pub fn collateral(&self) -> Collateral {
        self.collateral
    }

    /// Get the USD component
    pub fn usd(&self) -> Usd {
        self.usd
    }

    /// Add a new value
    pub fn checked_add_assign(
        &mut self,
        collateral: Collateral,
        price_point: &PricePoint,
    ) -> Result<()> {
        self.collateral = self.collateral.checked_add(collateral)?;
        self.usd = self
            .usd
            .checked_add(price_point.collateral_to_usd(collateral))?;
        Ok(())
    }
}

/// Collateral and USD which can become negative
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Copy, Default)]
pub struct SignedCollateralAndUsd {
    collateral: Signed<Collateral>,
    usd: Signed<Usd>,
}

impl SignedCollateralAndUsd {
    /// Create a new value from the given collateral
    pub fn new(collateral: Signed<Collateral>, price_point: &PricePoint) -> Self {
        SignedCollateralAndUsd {
            collateral,
            usd: collateral.map(|x| price_point.collateral_to_usd(x)),
        }
    }

    /// Get the collateral component
    pub fn collateral(&self) -> Signed<Collateral> {
        self.collateral
    }

    /// Get the USD component
    pub fn usd(&self) -> Signed<Usd> {
        self.usd
    }

    /// Add a new value
    pub fn checked_add_assign(
        &mut self,
        collateral: Signed<Collateral>,
        price_point: &PricePoint,
    ) -> Result<()> {
        self.collateral = self.collateral.checked_add(collateral)?;
        self.usd = self
            .usd
            .checked_add(collateral.map(|x| price_point.collateral_to_usd(x)))?;
        Ok(())
    }
}
