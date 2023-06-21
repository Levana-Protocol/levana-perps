use shared::storage::{MarketId, PriceBaseInQuote};

use crate::{
    endpoints::pnl::PositionInfo,
    types::{ChainId, ContractEnvironment, DirectionForDb},
};

/// Position data returned from the database for a given URL ID.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PositionInfoFromDb {
    pub(crate) market_id: String,
    pub(crate) environment: ContractEnvironment,
    pub(crate) pnl: String,
    pub(crate) direction: DirectionForDb,
    pub(crate) entry_price: String,
    pub(crate) exit_price: String,
    pub(crate) leverage: String,
    pub(crate) chain: ChainId,
}

/// Information sent to the database to insert a new position.
pub(crate) struct PositionInfoToDb {
    pub(crate) info: PositionInfo,
    pub(crate) market_id: MarketId,
    pub(crate) pnl: String,
    pub(crate) direction: DirectionForDb,
    pub(crate) entry_price: PriceBaseInQuote,
    pub(crate) exit_price: PriceBaseInQuote,
    pub(crate) leverage: String,
    pub(crate) environment: ContractEnvironment,
}
