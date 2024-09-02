use msg::contracts::market::{
    deferred_execution::DeferredExecId,
    entry::ClosedPositionCursor,
    position::{PositionId, PositionQueryResponse},
};
use shared::number::Usd;

use crate::prelude::*;

/// Total LP share information for a single market.
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub(crate) struct Totals {
    /// Total collateral still in this contract.
    ///
    /// Collateral used by active positions is excluded.
    pub(crate) collateral: Collateral,
    /// Total LP shares
    pub(crate) shares: LpToken,
    /// The last closed position we've collected collateral for.
    pub(crate) last_closed: Option<ClosedPositionCursor>,
    /// The latest deferred exec item we're waiting on.
    pub(crate) deferred_exec: Option<DeferredExecId>,
}

/// Information about positions in the market contract.
#[derive(Debug)]
pub(crate) enum PositionsInfo {
    TooManyPositions { to_close: PositionId },
    NoPositions,
    OnePosition { pos: Box<PositionQueryResponse> },
}

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) config: Config,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) my_addr: Addr,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct MarketInfo {
    pub(crate) id: MarketId,
    pub(crate) addr: Addr,
    pub(crate) token: msg::token::Token,
    pub(crate) crank_fee_surcharge: Usd,
    pub(crate) crank_fee_charged: Usd
}
