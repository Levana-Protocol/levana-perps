//! Provides specialized types that define a ratio that must be within a specific range.
//! This can be helpful when defining an interface that contains a ratio represented by a
//! decimal but the ratio is logically constrained by specific bounds.

use crate::prelude::cw_serde;
use anyhow::{ensure, Result};
use cosmwasm_std::Decimal256;
use std::ops::Bound;

fn validate_ratio(
    value: &Decimal256,
    lower: Bound<Decimal256>,
    upper: Bound<Decimal256>,
) -> Result<()> {
    let is_valid = (match lower {
        Bound::Included(lower) => lower <= *value,
        Bound::Excluded(lower) => lower < *value,
        Bound::Unbounded => true,
    }) && (match upper {
        Bound::Included(upper) => *value <= upper,
        Bound::Excluded(upper) => *value < upper,
        Bound::Unbounded => true,
    });

    ensure!(
        is_valid,
        "Invalid ratio, {} is out of range ({:?}, {:?})",
        value,
        lower,
        upper
    );

    Ok(())
}

#[cw_serde]
/// Represents a ratio between 0 and 1 inclusive
pub struct InclusiveRatio(Decimal256);

impl InclusiveRatio {
    /// Create a new InclusiveRatio
    pub fn new(value: Decimal256) -> Result<Self> {
        validate_ratio(
            &value,
            Bound::Included(Decimal256::zero()),
            Bound::Included(Decimal256::one()),
        )?;

        Ok(InclusiveRatio(value))
    }

    /// Get the underlying raw value.
    pub fn raw(&self) -> Decimal256 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounded_ratio() {
        // InclusiveRatio

        let raw_ratio = Decimal256::zero();
        InclusiveRatio::new(raw_ratio).unwrap();

        let raw_ratio = Decimal256::one();
        InclusiveRatio::new(raw_ratio).unwrap();

        let raw_ratio = Decimal256::from_ratio(1u64, 2u64);
        InclusiveRatio::new(raw_ratio).unwrap();

        let raw_ratio = Decimal256::from_ratio(2u64, 1u64);
        InclusiveRatio::new(raw_ratio).unwrap_err();
    }
}
