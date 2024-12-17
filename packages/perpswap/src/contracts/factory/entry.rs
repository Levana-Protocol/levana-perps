//! Entrypoint messages for the factory
use crate::contracts::market::entry::NewCounterTradeParams;
use crate::prelude::*;
use crate::{
    contracts::market::entry::{NewCopyTradingParams, NewMarketParams},
    shutdown::{ShutdownEffect, ShutdownImpact},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use cw_storage_plus::{KeyDeserialize, Prefixer, PrimaryKey};
use schemars::JsonSchema;

/// Instantiate a new factory contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// The code id for the market contract
    pub market_code_id: String,
    /// The code id for the position_token contract
    pub position_token_code_id: String,
    /// The code id for the liquidity_token contract
    pub liquidity_token_code_id: String,
    /// The code id for the copy trading contract
    pub copy_trading_code_id: Option<String>,
    /// The code id for the countertrade contract
    pub counter_trade_code_id: Option<String>,
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

    /// Change the migration admin
    SetMigrationAdmin {
        /// New migration admin
        migration_admin: RawAddr,
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

    /// Register a referrer for the given account.
    ///
    /// Can only be performed once.
    RegisterReferrer {
        /// The wallet address of the referrer
        addr: RawAddr,
    },
    /// Add new copy trading contract
    AddCopyTrading {
        /// Parameters for the contract
        new_copy_trading: NewCopyTradingParams,
    },
    /// Add new countertrade contract
    AddCounterTrade {
        /// Parameters for the contract
        new_counter_trade: NewCounterTradeParams,
    },
    /// Set the copy trading code id, i.e. if it's been migrated
    SetCopyTradingCodeId {
        /// Code ID to use for future copy trading contracts
        code_id: String,
    },
    /// Set the counter trade code id, i.e. if it's been migrated
    SetCounterTradeCodeId {
        /// Code ID to use for future countertrade contracts
        code_id: String,
    },
    /// Remove the owner from factory
    RemoveOwner {},
}

/// Response from [QueryMsg::Markets]
///
/// Use [QueryMsg::MarketInfo] for details on each market.
#[cw_serde]
pub struct MarketsResp {
    /// Markets maintained by this factory
    pub markets: Vec<MarketId>,
}

/// Response from [QueryMsg::Markets]
///
/// Use [QueryMsg::CopyTrading] for details on copy trading contract.
#[cw_serde]
pub struct CopyTradingResp {
    /// Copy trading contracts maintained by this factory
    pub addresses: Vec<CopyTradingInfo>,
}

/// Response from [QueryMsg::Markets]
///
/// Use [QueryMsg::CopyTrading] for details on copy trading contract.
#[cw_serde]
pub struct CounterTradeResp {
    /// Copy trading contracts maintained by this factory
    pub addresses: Vec<CounterTradeInfo>,
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
    /// Copy trading contract
    CopyTrading,
    /// Countertrade contract
    CounterTrade,
}

/// Default limit for [QueryMsg::Markets]
pub const MARKETS_QUERY_LIMIT_DEFAULT: u32 = 15;

/// Default limit for queries.
pub const QUERY_LIMIT_DEFAULT: u32 = 15;

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

    /// * returns [CodeIds]
    #[returns(CodeIds)]
    CodeIds {},

    /// Who referred this user, if anyone?
    ///
    /// * returns [GetReferrerResp]
    #[returns(GetReferrerResp)]
    GetReferrer {
        /// Referee address
        addr: RawAddr,
    },

    /// Enumerated query: who was referred by this user?
    ///
    /// * returns [ListRefereesResp]
    #[returns(ListRefereesResp)]
    ListReferees {
        /// Referrer address
        addr: RawAddr,
        /// How many addresses to return at once
        limit: Option<u32>,
        /// Taken from [ListRefereesResp::next_start_after]
        start_after: Option<String>,
    },

    /// Enumerated query: referee counts for all referrers.
    ///
    /// * returns [ListRefereeCountResp]
    #[returns(ListRefereeCountResp)]
    ListRefereeCount {
        /// How many records to return at once
        limit: Option<u32>,
        /// Take from [ListRefereeCountResp::next_start_after]
        start_after: Option<ListRefereeCountStartAfter>,
    },

    /// Fetch copy trading contracts
    ///
    /// Returns [CopyTradingResp]
    #[returns(CopyTradingResp)]
    CopyTrading {
        /// Last seen [CopyTradingInfo] in a [CopyTradingResp] for enumeration
        start_after: Option<CopyTradingInfoRaw>,
        /// Defaults to [QUERY_LIMIT_DEFAULT]
        limit: Option<u32>,
    },
    /// Fetch copy trading contract belonging to a specfic leader
    ///
    /// Returns [CopyTradingResp]
    #[returns(CopyTradingResp)]
    CopyTradingForLeader {
        /// Leader of the contract
        leader: RawAddr,
        /// Last seen copy trading contract address for enumeration
        start_after: Option<RawAddr>,
        /// Defaults to [QUERY_LIMIT_DEFAULT]
        limit: Option<u32>,
    },
    /// Fetch counter trade contracts
    #[returns(CounterTradeResp)]
    CounterTrade {
        /// Last seen [MarketId] in a [CounterTradeResp] for enumeration
        start_after: Option<MarketId>,
        /// Defaults to [QUERY_LIMIT_DEFAULT]
        limit: Option<u32>,
    },
}

/// Information on owners and other protocol-wide special addresses
#[cw_serde]
pub struct FactoryOwnerResp {
    /// Owner of the factory
    pub owner: Option<Addr>,
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
}

/// Return value from [QueryMsg::ShutdownStatus]
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
            ExecuteMsg::SetCounterTradeCodeId { .. } => true,
            ExecuteMsg::SetOwner { .. } => true,
            ExecuteMsg::SetMigrationAdmin { .. } => true,
            ExecuteMsg::SetDao { .. } => true,
            ExecuteMsg::SetKillSwitch { .. } => true,
            ExecuteMsg::SetWindDown { .. } => true,
            ExecuteMsg::TransferAllDaoFees {} => true,
            ExecuteMsg::RegisterReferrer { .. } => false,
            ExecuteMsg::AddCounterTrade { .. } => true,
            // Uses its own auth mechanism internally
            ExecuteMsg::Shutdown { .. } => false,
            ExecuteMsg::AddCopyTrading { .. } => false,
            ExecuteMsg::SetCopyTradingCodeId { .. } => true,
            ExecuteMsg::RemoveOwner {} => true,
        }
    }
}

/// Which code IDs are currently set for new markets
#[cw_serde]
pub struct CodeIds {
    /// Market code ID
    pub market: Uint64,
    /// Position token proxy code ID
    pub position_token: Uint64,
    /// Liquidity token proxy code ID
    pub liquidity_token: Uint64,
}

/// Response from [QueryMsg::GetReferrer]
#[cw_serde]
pub enum GetReferrerResp {
    /// No referrer registered
    NoReferrer {},
    /// Has a registered referrer
    HasReferrer {
        /// Referrer address
        referrer: Addr,
    },
}

/// Response from [QueryMsg::ListReferees]
#[cw_serde]
pub struct ListRefereesResp {
    /// Next batch of referees
    pub referees: Vec<Addr>,
    /// Next value to start after
    ///
    /// Returns `None` if we've seen all referees
    pub next_start_after: Option<String>,
}

/// Make a lookup key for the given referee
///
/// We don't follow the normal Map pattern to simplify raw queries.
pub fn make_referrer_key(referee: &Addr) -> String {
    format!("ref__{}", referee.as_str())
}

/// Make a lookup key for the count of referees for a referrer.
///
/// We don't follow the normal Map pattern to simplify raw queries.
pub fn make_referee_count_key(referrer: &Addr) -> String {
    format!("refcount__{}", referrer.as_str())
}

/// Response from [QueryMsg::ListRefereeCount]
#[cw_serde]
pub struct ListRefereeCountResp {
    /// Counts for individual wallets
    pub counts: Vec<RefereeCount>,
    /// Next value to start after
    ///
    /// Returns `None` if we've seen all referees
    pub next_start_after: Option<ListRefereeCountStartAfter>,
}

/// The count of referees for an individual referrer.
#[cw_serde]
pub struct RefereeCount {
    /// Referrer address
    pub referrer: Addr,
    /// Number of referees
    pub count: u32,
}

/// Helper for enumerated referee count queries.
#[cw_serde]
pub struct ListRefereeCountStartAfter {
    /// Last referrer seen.
    pub referrer: RawAddr,
    /// Last count seen.
    pub count: u32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, JsonSchema, PartialEq, Debug)]
/// Leader address
pub struct LeaderAddr(pub Addr);

#[derive(Clone, serde::Serialize, serde::Deserialize, JsonSchema, PartialEq, Debug)]
/// Copy trading address
pub struct CopyTradingAddr(pub Addr);

#[derive(Clone, serde::Serialize, serde::Deserialize, JsonSchema, PartialEq, Debug)]
/// Copy trading contract information
pub struct CopyTradingInfo {
    /// Leader of the contract
    pub leader: LeaderAddr,
    /// Address of the copy trading contract
    pub contract: CopyTradingAddr,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, JsonSchema, PartialEq, Debug)]
/// Copy trading contract information
pub struct CounterTradeInfo {
    /// Address of the counter trade contract
    pub contract: CounterTradeAddr,
    /// Associated market id of the counter trade contract
    pub market_id: MarketId
}

#[derive(Clone, serde::Serialize, serde::Deserialize, JsonSchema, PartialEq, Debug)]
/// Same as [CopyTradingInfo], but has raw addresses
pub struct CopyTradingInfoRaw {
    /// Leader of the contract
    pub leader: RawAddr,
    /// Address of the copy trading contract
    pub contract: RawAddr,
}

impl KeyDeserialize for LeaderAddr {
    type Output = LeaderAddr;

    const KEY_ELEMS: u16 = Addr::KEY_ELEMS;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        Addr::from_vec(value).map(LeaderAddr)
    }
}

impl<'a> Prefixer<'a> for LeaderAddr {
    fn prefix(&self) -> Vec<cw_storage_plus::Key> {
        self.0.prefix()
    }
}

impl<'a> PrimaryKey<'a> for LeaderAddr {
    type Prefix = <Addr as PrimaryKey<'a>>::Prefix;
    type SubPrefix = <Addr as PrimaryKey<'a>>::SubPrefix;
    type Suffix = <Addr as PrimaryKey<'a>>::Suffix;
    type SuperSuffix = <Addr as PrimaryKey<'a>>::SuperSuffix;

    fn key(&self) -> Vec<cw_storage_plus::Key> {
        self.0.key()
    }
}

impl KeyDeserialize for CopyTradingAddr {
    type Output = CopyTradingAddr;

    const KEY_ELEMS: u16 = Addr::KEY_ELEMS;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        Addr::from_vec(value).map(CopyTradingAddr)
    }
}
impl<'a> Prefixer<'a> for CopyTradingAddr {
    fn prefix(&self) -> Vec<cw_storage_plus::Key> {
        self.0.prefix()
    }
}

impl<'a> PrimaryKey<'a> for CopyTradingAddr {
    type Prefix = <Addr as PrimaryKey<'a>>::Prefix;
    type SubPrefix = <Addr as PrimaryKey<'a>>::SubPrefix;
    type Suffix = <Addr as PrimaryKey<'a>>::Suffix;
    type SuperSuffix = <Addr as PrimaryKey<'a>>::SuperSuffix;

    fn key(&self) -> Vec<cw_storage_plus::Key> {
        self.0.key()
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize, JsonSchema, PartialEq, Debug)]
/// Counter trade contract address
pub struct CounterTradeAddr(pub Addr);

impl KeyDeserialize for CounterTradeAddr {
    type Output = CounterTradeAddr;

    const KEY_ELEMS: u16 = Addr::KEY_ELEMS;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        Addr::from_vec(value).map(CounterTradeAddr)
    }
}
impl<'a> Prefixer<'a> for CounterTradeAddr {
    fn prefix(&self) -> Vec<cw_storage_plus::Key> {
        self.0.prefix()
    }
}

impl<'a> PrimaryKey<'a> for CounterTradeAddr {
    type Prefix = <Addr as PrimaryKey<'a>>::Prefix;
    type SubPrefix = <Addr as PrimaryKey<'a>>::SubPrefix;
    type Suffix = <Addr as PrimaryKey<'a>>::Suffix;
    type SuperSuffix = <Addr as PrimaryKey<'a>>::SuperSuffix;

    fn key(&self) -> Vec<cw_storage_plus::Key> {
        self.0.key()
    }
}
