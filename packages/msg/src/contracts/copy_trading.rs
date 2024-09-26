//! Copy trading contract

use std::{fmt::Display, num::ParseIntError, str::FromStr};

use anyhow::anyhow;
use cosmwasm_std::{Addr, Binary, Decimal256, StdError, StdResult, Uint128, Uint64};
use cw_storage_plus::{IntKey, Key, KeyDeserialize, Prefixer, PrimaryKey};
use shared::{
    number::{Collateral, LpToken, NonZero, Usd},
    storage::{MarketId, RawAddr},
    time::Timestamp,
};

/// Message for instantiating a new copy trading contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Factory contract that trading will be performed on
    pub factory: RawAddr,
    /// Address of the administrator of the contract
    pub admin: RawAddr,
    /// Leader of the contract
    pub leader: RawAddr,
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
    /// which initializes this.
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
    /// Commission rate for the leader. Should be within 1-30%.
    pub commission_rate: Decimal256,
    /// Creation time of contract
    pub created_at: Timestamp,
}

impl Config {
    /// Check validity of config values
    pub fn check(&self) -> anyhow::Result<()> {
        if self.name.len() > 128 {
            Err(anyhow!(
                "Description should not be more than 128 characters"
            ))
        } else if self.commission_rate < Decimal256::from_ratio(1u32, 100u32) {
            Err(anyhow!("Commission rate less than 1 percent"))
        } else if self.commission_rate > Decimal256::from_ratio(30u32, 100u32) {
            Err(anyhow!("Commission rate greater than 1 percent"))
        } else {
            Ok(())
        }
    }
}

/// Updates to configuration values.
///
/// See [Config] for field meanings.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub struct ConfigUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub commission_rate: Option<Decimal256>,
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
    Deposit {},
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
    /// Leader specific execute messages
    LeaderMsg {
        /// Market id that message is for
        market_id: MarketId,
        /// Message
        message: LeaderExecuteMsg,
    },
    /// Perform queue work
    DoWork {},
}

/// Market specific execution for Leader
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LeaderExecuteMsg {
    /// Open position and various other messages
    OpenPosition {},
    /// Close position etc
    ClosePosition {},
}

/// Queries that can be performed on the copy contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Get the current config
    ///
    /// Returns [Config]
    Config {},
    /// Get the queue status of a particular wallet
    ///
    /// Returns [QueueResp]
    QueueStatus {
        /// Address of the wallet
        address: RawAddr,
        /// Value from []
        start_after: Option<QueuePositionId>,
        /// How many values to return
        limit: Option<u32>,
    },
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
    /// Does it has any pending work ?
    ///
    /// Returns [WorkResp]
    HasWork {},
}

/// Individual response from [QueryMsg::QueueStatus]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct QueueResp {
    /// Items in queue for the wallet
    pub items: Vec<QueueRespItem>,
    /// Last processed [QueuePositionId]
    pub processed_till: Option<QueuePositionId>,
}

/// Queue Item
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct QueueRespItem {
    /// Queue position id
    pub id: QueuePositionId,
    /// Item in the queue corresponding to the [QueuePositionId]
    pub item: QueueItem,
}

/// Queue item that needs to be processed
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum QueueItem {
    /// Deposit the fund and get some [LpToken]
    Deposit {
        /// Funds to be deposited
        funds: NonZero<Collateral>,
        /// Token
        token: Token,
    },
    /// Withdraw via LpToken
    Withdrawal {
        /// Tokens to be withdrawn
        tokens: NonZero<LpToken>,
        /// Token type
        token: Token,
    },
    /// Open Position etc. etc.
    OpenPosition {},
}

/// Token required for the queue item
pub enum RequiresToken {
    /// Token required
    Token {
        /// Token
        token: Token,
    },
    /// Token not requird
    NoToken {},
}

impl QueueItem {
    /// Does this queue item require computation of LP token value
    pub fn requires_token(self) -> RequiresToken {
        match self {
            QueueItem::Deposit { token, .. } => RequiresToken::Token { token },
            QueueItem::Withdrawal { token, .. } => RequiresToken::Token { token },
            QueueItem::OpenPosition {} => RequiresToken::NoToken {},
        }
    }
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

impl Token {
    /// Is it same as market token ?
    pub fn is_same(&self, token: &crate::token::Token) -> bool {
        match token {
            crate::token::Token::Cw20 { addr, .. } => match self {
                Token::Native(_) => false,
                Token::Cw20(cw20_addr) => {
                    let cw20_addr: &RawAddr = &cw20_addr.into();
                    cw20_addr == addr
                }
            },
            crate::token::Token::Native { denom, .. } => match self {
                Token::Native(native_denom) => *native_denom == *denom,
                Token::Cw20(_) => false,
            },
        }
    }
}

impl<'a> PrimaryKey<'a> for Token {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        let (token_type, bytes) = match self {
            Token::Native(native) => (0u8, native.as_bytes()),
            Token::Cw20(cw20) => (1u8, cw20.as_bytes()),
        };
        let token_type = Key::Val8([token_type]);
        let key = Key::Ref(bytes);

        vec![token_type, key]
    }
}

impl<'a> Prefixer<'a> for Token {
    fn prefix(&self) -> Vec<Key> {
        let (token_type, bytes) = match self {
            Token::Native(native) => (0u8, native.as_bytes()),
            Token::Cw20(cw20) => (1u8, cw20.as_bytes()),
        };
        let token_type = Key::Val8([token_type]);
        let key = Key::Ref(bytes);
        vec![token_type, key]
    }
}

impl KeyDeserialize for Token {
    type Output = Token;

    const KEY_ELEMS: u16 = 2;

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        let keys = value.key();
        if keys.len() != 2 {
            return Err(StdError::serialize_err("Token", "Token len is not two"));
        }
        let token_type = &keys[0];
        let token = keys[1].as_ref();
        let token_type = match token_type {
            Key::Val8([token_type]) => token_type,
            _ => return Err(StdError::serialize_err("Token", "Invalid token type")),
        };
        let token = match token_type {
            0 => {
                let native_token = String::from_slice(token)?;
                Token::Native(native_token)
            }
            1 => {
                let cw20_token = Addr::from_slice(token)?;
                Token::Cw20(cw20_token)
            }
            _ => {
                return Err(StdError::serialize_err(
                    "Token",
                    "Invalid number in token_type",
                ))
            }
        };
        Ok(token)
    }
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkDescription {
    /// Calculate LP token value
    ComputeLpTokenValue {
        /// Token
        token: Token,
    },
    /// Process market
    ProcessMarket {
        /// Market id
        id: MarketId,
    },
    /// Process Queue item
    ProcessQueueItem {
        /// Id to process
        id: QueuePositionId,
    },
    /// Reset market specific statistics
    ResetStats {},
    /// Rebalance for case when someone sends collateral directly to
    /// the contract without getting LpTokens
    Rebalance {},
}

/// Queue position number
#[derive(
    Copy, PartialOrd, Ord, Eq, Clone, PartialEq, serde::Serialize, serde::Deserialize, Debug,
)]
pub struct QueuePositionId(Uint64);

impl QueuePositionId {
    /// Construct a new value from a [u64].
    pub fn new(x: u64) -> Self {
        QueuePositionId(x.into())
    }

    /// The underlying `u64` representation.
    pub fn u64(self) -> u64 {
        self.0.u64()
    }

    /// Generate the next position ID
    ///
    /// Panics on overflow
    pub fn next(self) -> Self {
        QueuePositionId((self.u64() + 1).into())
    }
}

impl<'a> PrimaryKey<'a> for QueuePositionId {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.u64().to_cw_bytes())]
    }
}

impl<'a> Prefixer<'a> for QueuePositionId {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.u64().to_cw_bytes())]
    }
}

impl KeyDeserialize for QueuePositionId {
    type Output = QueuePositionId;

    const KEY_ELEMS: u16 = 1;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        u64::from_vec(value).map(|x| QueuePositionId(Uint64::new(x)))
    }
}

impl std::fmt::Display for QueuePositionId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for QueuePositionId {
    type Err = ParseIntError;
    fn from_str(src: &str) -> Result<Self, ParseIntError> {
        src.parse().map(|x| QueuePositionId(Uint64::new(x)))
    }
}

/// Migration message, currently no fields needed
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}
