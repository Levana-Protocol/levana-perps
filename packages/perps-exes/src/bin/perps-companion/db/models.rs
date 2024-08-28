use cosmwasm_std::Uint64;
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

/// Proposal data returned from the database for a given URL ID.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProposalInfoFromDb {
    pub(crate) title: String,
    pub(crate) environment: ContractEnvironment,
    pub(crate) chain: ChainId,
}

/// Information sent to the database to insert a new proposal.
pub(crate) struct ProposalInfoToDb {
    pub(crate) proposal_id: u64,
    pub(crate) title: String,
    pub(crate) environment: ContractEnvironment,
    pub(crate) chain: ChainId,
}
