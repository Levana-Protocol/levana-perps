//! Copy trading contract

use std::fmt::Display;

use cosmwasm_std::{Addr, Binary, Decimal256, Uint128, Uint64};
use shared::{
    number::{Collateral, LpToken, NonZero, Signed, Usd},
    storage::{MarketId, RawAddr},
    time::Timestamp,
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
    /// Administrator of the contract. Should be the factory contract
    /// which initializes this as that will migrate this contract.
    pub admin: Addr,
    /// Pending administrator, ready to be accepted, if any.
    pub pending_admin: Option<Addr>,
    /// Leader of the contract
    pub leader: Addr,
    /// Name given to this copy_trading pool
    pub name: String,
    /// Description of the copy_trading pool. Not more than 128
    /// characters.
    pub description: String,
    /// Commission rate for the leader. Only paid when trade is
    /// profitable.
    pub commission_rate: Decimal256,
    /// Creation time of contract
    pub created_at: Timestamp
}

/// Updates to configuration values.
///
/// See [Config] for field meanings.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub struct ConfigUpdate {
    pub name: String,
    pub description: String,
    pub commission_rate: Decimal256,
    pub min_balance: NonZero<Collateral>,
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
    /// Deposit funds to the contract
    Deposit {
        /// Token being deposited
        token: crate::token::Token
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
    /// Wind down the contract which prevents opening any new
    /// positions, or deposits or collateral.
    WindDown {},
    /// Shutdown the contract by closing all the open positions etc.
    Shutdown {},
    /// Open position and various other messages
    OpenPosition {},
}

/// Queries that can be performed on the copy contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Get the current config
    ///
    /// Returns [Config]
    Config {},
    /// Returns the share held by the wallet
    ///
    /// Returns [BalanceResp]
    Balance {
        /// Address of the token holder
        address: RawAddr,
    },
    /// Check the status of the copy trading contract for all the
    /// markets that it's trading on
    ///
    /// Returns [StatusResp]
    Status {
        /// Value from [BalanceResp::next_start_after]
        start_after: Option<MarketId>,
        /// How many values to return
        limit: Option<u32>,
    },
    /// Returns all the open positions
    ///
    /// Returns [OpenPositionsResp]
    OpenPositions {
        /// Value from [OpenPositionResp::next_start_after]
        start_after: Option<PositionId>,
        /// How many values to return
        limit: Option<u32>,
    },
    /// Does it has any pending work ?
    ///
    /// Returns [WorkResp]
    HasWork {},
}
// todo: Also implement query for open orders, closed orders etc.

/// Response from [QueryMsg::OpenPositionsResp]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct OpenPositionsResp {
    /// Market balances in this batch
    pub positions: Vec<PositionQueryResponse>,
    /// Next start_after value, if we have more leaders
    pub next_start_after: Option<PositionId>,
}

/// Individual market response from [QueryMsg::Status]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct StatusResp {
    /// Market id
    pub market_id: MarketId,
    /// Sum of deposit collateral of all open positions
    pub tvl_open_positions_usd: Usd,
    /// Sum of deposit collateral of all closed positions
    pub tvl_closed_positions_usd: Usd,
    /// Total profit so far in the closed positions
    pub profit_in_usd: Usd,
    /// Total loss so far in the closed postions
    pub loss_in_usd: Usd,
}

/// Individual market response from [QueryMsg::Balance]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct BalanceResp {
    /// Shares of the pool held by the wallet
    pub shares: NonZero<LpToken>,
}

/// Token accepted by the contract
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Token {
    /// Native coin and its denom
    Native(String),
    /// CW20 contract and its address
    Cw20(Addr),
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Token::Native(denom) => f.write_str(denom),
            Token::Cw20(addr) => f.write_str(addr.as_str()),
        }
    }
}

impl Token {
    /// Ensure that the two versions of the token are compatible.
    pub fn ensure_matches(&self, token: &crate::token::Token) -> anyhow::Result<()> {
        match (self, token) {
            (Token::Native(_), crate::token::Token::Cw20 { addr, .. }) => {
                anyhow::bail!("Provided native funds, but market requires a CW20 (contract {addr})")
            }
            (
                Token::Native(denom1),
                crate::token::Token::Native {
                    denom: denom2,
                    decimal_places: _,
                },
            ) => {
                if denom1 == denom2 {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Wrong denom provided. You sent {denom1}, but the contract expects {denom2}"))
                }
            }
            (
                Token::Cw20(addr1),
                crate::token::Token::Cw20 {
                    addr: addr2,
                    decimal_places: _,
                },
            ) => {
                if addr1.as_str() == addr2.as_str() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "Wrong CW20 used. You used {addr1}, but the contract expects {addr2}"
                    ))
                }
            }
            (Token::Cw20(_), crate::token::Token::Native { denom, .. }) => {
                anyhow::bail!(
                    "Provided CW20 funds, but market requires native funds with denom {denom}"
                )
            }
        }
    }
}

/// Work response
pub enum WorkResp {
    /// No work found
    NoWork,
    /// Has some work
    HasWork {
        /// Work description
        work_description: WorkDescription,
    },
}

/// Work Description
pub enum WorkDescription {
    /// Calculate LP token value
    ComputeLpTokenValue {},
    /// Process market
    ProcessMarket {
        /// Market id
        id: MarketId
    },
    /// Process Queue item
    ProcessQueueItem {
        /// Id to process
        id: QueuePositionId,
    },
    /// Process Earmark item for withdrawals
    ProcessEarmarkItem {
        /// Id to process
        id: EarmarkId,
    },
    /// Reset market specific statistics
    RestStats {}
}

/// Queue position number
pub struct QueuePositionId(Uint64);

/// Earmark Id
pub struct EarmarkId(Uint64);
