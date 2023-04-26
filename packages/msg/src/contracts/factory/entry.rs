//! Entrypoint messages for the factory
use crate::{
    contracts::market::entry::NewMarketParams,
    shutdown::{ShutdownEffect, ShutdownImpact},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use shared::prelude::*;

/// Instantiate a new factory contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// The code id for the market contract
    pub market_code_id: String,
    /// The code id for the position_token contract
    pub position_token_code_id: String,
    /// The code id for the liquidity_token contract
    pub liquidity_token_code_id: String,
    /// Migration admin, needed for instantiating/migrating sub-contracts
    pub migration_admin: RawAddr,
    /// Perpetual swap admin address
    pub owner: RawAddr,
    /// DAO address
    pub dao: RawAddr,
    /// Kill switch address
    pub kill_switch: RawAddr,
    /// Wind down address
    pub wind_down: RawAddr,
    /// Suffix attached to all contracts instantiated by the factory
    pub label_suffix: Option<String>,
}

/// Execute a message on the factory.
#[allow(clippy::large_enum_variant)]
#[cw_serde]
pub enum ExecuteMsg {
    /// Add a new market
    AddMarket {
        /// Parameters for the new market
        new_market: NewMarketParams,
    },
    /// Set the market code id, i.e. if it's been migrated
    SetMarketCodeId {
        /// Code ID to use for future market contracts
        code_id: String,
    },
    /// Set the position token code id, i.e. if it's been migrated
    SetPositionTokenCodeId {
        /// Code ID to use for future position token contracts
        code_id: String,
    },
    /// Set the liquidity token code id, i.e. if it's been migrated
    SetLiquidityTokenCodeId {
        /// Code ID to use for future liquidity token contracts
        code_id: String,
    },

    /// Change the owner addr
    SetOwner {
        /// New owner
        owner: RawAddr,
    },

    /// Change the dao addr
    SetDao {
        /// New DAO
        dao: RawAddr,
    },

    /// Change the kill switch addr
    SetKillSwitch {
        /// New kill switch administrator
        kill_switch: RawAddr,
    },

    /// Change the wind down addr
    SetWindDown {
        /// New wind down administrator
        wind_down: RawAddr,
    },

    /// Set market price admin addr for a given market
    SetMarketPriceAdmin {
        /// The market contract addr whose market price can be updated by the specified admin
        market_addr: RawAddr,
        /// The admin addr that is allowed to update the market price for the specified market
        admin_addr: RawAddr,
    },

    /// Convenience mechanism to transfer all dao fees from all markets
    TransferAllDaoFees {},

    /// Perform a shutdown on the given markets with the given impacts
    Shutdown {
        /// Which markets to impact? Empty list means impact all markets
        markets: Vec<MarketId>,
        /// Which impacts to have? Empty list means shut down all activities
        impacts: Vec<ShutdownImpact>,
        /// Are we disabling these impacts, or reenabling them?
        effect: ShutdownEffect,
    },
}

/// Response from [QueryMsg::Markets]
///
/// Use [QueryMsg::MarketInfo] for details on each market.
#[cw_serde]
pub struct MarketsResp {
    /// Markets maintained by this factory
    pub markets: Vec<MarketId>,
}

/// Response from [QueryMsg::AddrIsContract]
#[cw_serde]
pub struct AddrIsContractResp {
    /// Boolean indicating whether this is a success for failure.
    pub is_contract: bool,
    /// If this is a contract: what type of contract is it?
    pub contract_type: Option<ContractType>,
}

/// The type of contract identified by [QueryMsg::AddrIsContract].
#[cw_serde]
pub enum ContractType {
    /// The factory contract
    Factory,
    /// An LP or xLP liquidity token proxy
    LiquidityToken,
    /// A position NFT proxy
    PositionToken,
    /// A market
    Market,
}

/// Default limit for [QueryMsg::Markets]
pub const MARKETS_QUERY_LIMIT_DEFAULT: u32 = 15;

/// Queries available on the factory contract
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [cw2::ContractVersion]
    #[returns(cw2::ContractVersion)]
    Version {},

    /// * returns [MarketsResp]
    ///
    /// All the markets
    #[returns(MarketsResp)]
    Markets {
        /// Last seen market ID in a [MarketsResp] for enumeration
        start_after: Option<MarketId>,
        /// Defaults to [MARKETS_QUERY_LIMIT_DEFAULT]
        limit: Option<u32>,
    },

    /// * returns [MarketInfoResponse]
    ///
    /// Combined query to get the market related addresses
    #[returns(MarketInfoResponse)]
    MarketInfo {
        /// Market ID to look up
        market_id: MarketId,
    },

    /// * returns [AddrIsContractResp]
    ///
    /// given an address, checks if it's any of the registered protocol contracts.
    #[returns(AddrIsContractResp)]
    AddrIsContract {
        /// Address to check
        addr: RawAddr,
    },

    /// * returns [FactoryOwnerResp]
    ///
    /// Returns information about the owners of the factory
    #[returns(FactoryOwnerResp)]
    FactoryOwner {},

    /// * returns [ShutdownStatus]
    #[returns(ShutdownStatus)]
    ShutdownStatus {
        /// Market to look up
        market_id: MarketId,
    },
}

/// Information on owners and other protocol-wide special addresses
#[cw_serde]
pub struct FactoryOwnerResp {
    /// Owner of the factory
    pub owner: Addr,
    /// Migration admin of the factory
    pub admin_migration: Addr,
    /// Wallet that receives DAO/protocol fees for all markets
    pub dao: Addr,
    /// Wallet that can activate kill switch shutdowns
    pub kill_switch: Addr,
    /// Wallet that can activate market wind downs
    pub wind_down: Addr,
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}

/// Information about a specific market, returned from [QueryMsg::MarketInfo].
#[cw_serde]
pub struct MarketInfoResponse {
    /// Address of the market
    pub market_addr: Addr,
    /// Address of the position token
    pub position_token: Addr,
    /// Address of the LP liquidity token
    pub liquidity_token_lp: Addr,
    /// Address of the xLP liquidity token
    pub liquidity_token_xlp: Addr,
    /// Address of the price admin
    pub price_admin: Addr,
}

/// Return value from [QueryMsg::Shutdown]
#[cw_serde]
pub struct ShutdownStatus {
    /// Any parts of the market which have been disabled.
    pub disabled: Vec<ShutdownImpact>,
}

impl ExecuteMsg {
    /// Does this message require owner permissions?
    pub fn requires_owner(&self) -> bool {
        match self {
            ExecuteMsg::AddMarket { .. } => true,
            ExecuteMsg::SetMarketCodeId { .. } => true,
            ExecuteMsg::SetPositionTokenCodeId { .. } => true,
            ExecuteMsg::SetLiquidityTokenCodeId { .. } => true,
            ExecuteMsg::SetOwner { .. } => true,
            ExecuteMsg::SetDao { .. } => true,
            ExecuteMsg::SetKillSwitch { .. } => true,
            ExecuteMsg::SetWindDown { .. } => true,
            ExecuteMsg::SetMarketPriceAdmin { .. } => true,
            ExecuteMsg::TransferAllDaoFees {} => true,
            // Uses its own auth mechanism internally
            ExecuteMsg::Shutdown { .. } => false,
        }
    }
}
