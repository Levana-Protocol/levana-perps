use msg::contracts::market::position::{PositionId, PositionQueryResponse};

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
}

/// Information about positions in the market contract.
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
}

/// Possible reply states
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) enum ReplyState {
    ClosingPositions {
        market: MarketId,
        previous_balance: Collateral,
    },
}
