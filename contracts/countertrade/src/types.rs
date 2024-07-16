use cosmwasm_std::{Empty, QuerierWrapper};

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

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) config: Config,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
}

pub(crate) struct MarketState<'a> {
    pub(crate) state: State<'a>,
    pub(crate) market: MarketInfo,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct MarketInfo {
    pub(crate) id: MarketId,
    pub(crate) token: msg::token::Token,
}
