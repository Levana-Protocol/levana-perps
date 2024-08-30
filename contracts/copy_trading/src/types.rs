use std::fmt::Display;

use msg::contracts::market::{deferred_execution::DeferredExecId, position::PositionId};
use shared::number::Usd;

use crate::prelude::*;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct MarketInfo {
    pub(crate) id: MarketId,
    pub(crate) addr: Addr,
    pub(crate) token: msg::token::Token,
}

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) config: Config,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) my_addr: Addr,
}

/// Total LP share information
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub(crate) struct Totals {
    /// Total collateral still in this contract.
    ///
    /// Collateral used by active positions is excluded.
    pub(crate) collateral: Collateral,
    /// Total LP shares
    pub(crate) shares: LpToken,
}

/// Total LP share information per market
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct MarketTotals {
    /// Collateral used by active positions in the collateral
    pub(crate) collateral: Collateral,
    /// Total LP shares represented by the locked collateral
    pub(crate) shares: LpToken,
    /// Positions associated with this market
    pub(crate) positions: MarketPositions
}

/// Market positions tracked by the contract
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub(crate) struct MarketPositions {
    /// Open positions that have been processed
    pub(crate) open_positions: Vec<PositionInfo>,
    /// Open positions that needs to be processed
    pub(crate) pending_open_positions: Vec<PositionId>,
    /// Positons that were updated and that needs to be processed
    pub(crate) pending_updated_positions: Vec<PositionId>,
    /// Closed position that are pending and needs to be processed
    pub(crate) pending_closed_positions: Vec<PositionId>
}

/// Specific position information
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct PositionInfo {
    /// Unique identifier for a position
    pub(crate) id: PositionId,
    /// Active collateral for the position
    pub(crate) active_collateral: NonZero<Collateral>,
    /// Unrealized PnL on this position, in terms of collateral.
    pub(crate) pnl_collateral: Signed<Collateral>,
    /// Unrealized PnL on this position, in USD, using cost-basis analysis.
    pub(crate) pnl_usd: Signed<Usd>
}
