//! Copy trading contract

use cosmwasm_std::{Addr, Binary, Decimal256, Uint128, Uint64};
use shared::{
    number::{Collateral, LpToken, NonZero, Signed},
    storage::RawAddr, time::Timestamp,
};

use super::market::position::{PositionId, PositionQueryResponse};

/// Message for instantiating a new copy trading contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Factory contract that trading will be performed on
    pub factory: RawAddr,
    /// Address of the administrator of the contract
    pub admin: RawAddr,
    /// Leader of the contract
    pub leader: Addr,
    /// Initial configuration values
    pub config: ConfigUpdate,
}

/// Full configuration
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
/// Updates to configuration values.
pub struct Config {
    /// Factory we will allow trading on
    pub factory: Addr,
    /// Administrator of the contract, allowed to make config updates
    pub admin: Addr,
    /// Pending administrator, ready to be accepted, if any.
    pub pending_admin: Option<Addr>,
    /// Leader of the contract
    pub leader: Addr,
    /// Name given to this copy_trading pool
    pub name: String,
    /// Kind of token accepted by this liqudity pool
    pub token: crate::token::Token,
    /// Commission rate for the leader. Only paid when trade is
    /// profitable.
    pub commission_rate: Decimal256,
    /// Minimum balance that the leader needs to maintain at all time
    pub min_balance: NonZero<Collateral>
}

/// Updates to configuration values.
///
/// See [Config] for field meanings.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub struct ConfigUpdate {
    pub name: String,
    pub token: crate::token::Token,
    pub commission_rate: Decimal256,
    pub min_balance: NonZero<Collateral>
}

/// Executions available on the copy trading contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Cw20 interface
    Receive {
        /// Owner of funds sent to the contract
        sender: RawAddr,
        /// Amount of funds sent
        amount: Uint128,
        /// Must parse to a [ExecuteMsg]
        msg: Binary,
    },
    /// Deposit funds for this liqudity pool
    Deposit {
        /// Which pool id should the deposit go to
        pool_id: PoolId,
    },
    /// Withdraw funds from a given market
    Withdraw {
        /// The number of LP shares to remove
        amount: NonZero<LpToken>,
    },
    /// Appoint a new administrator
    AppointAdmin {
        /// Address of the new administrator
        admin: RawAddr,
    },
    /// Accept appointment of admin
    AcceptAdmin {},
    /// Update configuration values
    UpdateConfig(ConfigUpdate),
    /// Open position etc
    OpenPosition {

    }
    /// Do work ?
}

/// Queries that can be performed on the copy contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Get the current config
    ///
    /// Returns [Config]
    Config {},
    /// Check the balance of an address for all pools.
    ///
    /// Returns [BalanceResp]
    Balance {
        /// Address of the token holder
        address: RawAddr,
        /// Value from [BalanceResp::next_start_after]
        start_after: Option<PoolId>,
        /// How many values to return
        limit: Option<u32>,
    },
    /// Check the status of all pools
    ///
    /// Returns [PoolResp]
    PoolStatus {
        /// Value from [MarketsResp::next_start_after]
        start_after: Option<PoolId>,
        /// Include closed pool ? By default they are not included.
        include_closed: Option<bool>,
        /// How many values to return
        limit: Option<u32>,
    },
    /// Returns all the leaders for the current token
    ///
    /// Returns [LeadersResp]
    Leaders {},
    /// Returns all the open positions
    ///
    /// Returns [OpenPositionsResp]
    OpenPositions {
        /// Pool id
        id: PoolId,
        /// Value from [OpenPositionResp::next_start_after]
        start_after: Option<PositionId>,
        /// How many values to return
        limit: Option<u32>,
    }
}
// todo: Also implement query for open orders, closed orders etc.

/// Response from [QueryMsg::OpenPositionsResp]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct OpenPositionsResp {
    /// Market balances in this batch
    pub positions: Vec<PositionQueryResponse>,
    /// Next start_after value, if we have more leaders
    pub next_start_after: Option<PositionId>
}

/// Response from [QueryMsg::Leaders]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct LeadersResp {
    /// Market balances in this batch
    pub leader: Vec<LeaderInfo>,
    /// Next start_after value, if we have more leaders
    pub next_start_after: Option<Addr>,
}

/// Pool Id
#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoolId(Uint64);

/// Individual market response from [QueryMsg::Leaders]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct LeaderInfo {
    /// Leader address
    pub leader: Addr,
    /// Shares helds by the leader
    pub shares: NonZero<LpToken>,
    /// Collateral equivalent of these shares
    pub collateral: NonZero<Collateral>,
    /// Size of the pool managed by the leader
    pub pool_size: NonZero<LpToken>,
    /// Pool id managed by the leader
    pub pool_id: PoolId,
    /// Pool name set by the leader
    pub pool_name: String,
    /// Is the pool closed ?
    pub pool_closed: bool,
}

/// Response from [QueryMsg::Balance]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct BalanceResp {
    /// Pool balances in this batch
    pub markets: Vec<PoolBalance>,
    /// Next start_after value, if we have more balances
    pub next_start_after: Option<crate::token::Token>,
}
/// Individual market response from [QueryMsg::Balance]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct PoolBalance {
    /// Pool id
    pub id: PoolId,
    /// Shares of the pool held by the wallet
    pub shares: NonZero<LpToken>,
    /// Collateral equivalent of these shares
    pub collateral: NonZero<Collateral>,
    /// Size of the entire pool, in LP tokens
    pub pool_size: NonZero<LpToken>,
}

/// Response from [QueryMsg::PoolStatus]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct PoolResp {
    /// Token statuses in this batch
    pub tokens: Vec<PoolStatus>,
    /// Next start_after value, if we have more tokens
    pub next_start_after: Option<crate::token::Token>,
}

/// Status of a single pool
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct PoolStatus {
    /// Pool id
    pub id: PoolId,
    /// Pool name
    pub name: String,
    /// Leader's wallet
    pub leader: Addr,
    /// Pool creation time
    pub created_at: Timestamp,
    /// Commission rate for the leader
    pub commission_rate: Decimal256,
    /// Collateral held inside the contract
    ///
    /// Does not include active collateral of positions
    pub collateral: Collateral,
    /// Number of outstanding shares
    pub shares: LpToken,
    /// Our open position collateral
    pub position_collateral: Signed<Collateral>,
    /// Realized profit and loss
    pub realized_pnl: Signed<Collateral>,
    /// Total trade volume
    pub total_traded_volume: Signed<Collateral>
}
