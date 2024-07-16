use crate::prelude::*;

/// Total LP share information for a single market.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Totals {
    /// Total collateral still in this contract.
    ///
    /// Collateral used by active positions is excluded.
    pub(crate) collateral: Collateral,
    /// Total LP shares
    pub(crate) shares: LpToken,
}
