//! Countertrade contract

use std::fmt::Display;

use cosmwasm_std::{Addr, Binary, Decimal256, Uint128};
use shared::storage::{Collateral, LeverageToBase, LpToken, MarketId, NonZero, RawAddr};

/// Countertrade-specific errrors.
#[derive(thiserror::Error, Debug)]
#[allow(missing_docs)]
pub enum Error {
    /// Wrap up underlying errors with context
    #[error("{context}: {source}")]
    Context {
        /// Underlying error, serialized to a string
        source: Box<dyn std::error::Error>,
        /// User-friendly contextual string
        context: String,
    },
    #[error("Invalid config found: {message}")]
    InvalidConfig {
        /// Message to display to the user
        message: String,
    },
    #[error("Invalid migration: {message}")]
    InvalidMigration {
        /// Message to display to the user
        message: String,
    },
    /// Funds were attached with a CW20 receive
    #[error("Cannot attach native funds when sending a CW20 receive")]
    FundsWithCw20,
    #[error("Cannot use a CW20 receive inside another receive")]
    ReceiveInsideReceive,
    #[error("Cannot accept multiple fund denoms")]
    MultipleNativeFunds,
    #[error("Funds were attached when not necessary: {amount}{token}")]
    UnnecessaryFunds { token: Token, amount: Uint128 },
    #[error("No funds were provided, this message requires attached funds")]
    MissingRequiredFunds,
}

/// A result with the error specialized to countertrade errors.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Message for instantiating a new countertrade contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Factory contract we're countertrading on
    pub factory: RawAddr,
    /// Address of the administrator of the contract
    pub admin: RawAddr,
    /// Initial configuration values
    pub config: ConfigUpdate,
}

/// Full configuration
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
/// Updates to configuration values.
pub struct Config {
    /// Administrator of the contract, allowed to make config updates
    pub admin: Addr,
    /// Pending administrator, ready to be accepted, if any.
    pub pending_admin: Option<Addr>,
    /// Factory we are balancing
    pub factory: Addr,
    /// Minimum funding rate for popular side
    pub min_funding: Decimal256,
    /// Target funding rate for popular side
    pub target_funding: Decimal256,
    /// Maximum funding rate for popular side
    pub max_funding: Decimal256,
    /// Maximum leverage value we'll use
    ///
    /// If a market has lower max leverage, we use that instead
    pub max_leverage: LeverageToBase,
}

impl Config {
    /// Check validity of config values
    pub fn check(&self) -> Result<()> {
        if self.min_funding >= self.target_funding {
            Err("Minimum funding must be strictly less than target")
        } else if self.target_funding >= self.max_funding {
            Err("Target funding must be strictly less than max")
        } else if self.max_leverage.into_decimal256() < Decimal256::from_ratio(2u32, 1u32) {
            Err("Max leverage must be at least 2")
        } else {
            Ok(())
        }
        .map_err(|s| Error::InvalidConfig {
            message: s.to_owned(),
        })
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
/// Updates to configuration values.
///
/// See [Config] for field meanings.
pub struct ConfigUpdate {
    pub min_funding: Option<Decimal256>,
    pub target_funding: Option<Decimal256>,
    pub max_funding: Option<Decimal256>,
    pub max_leverage: Option<LeverageToBase>,
}

/// Executions available on the countertrade contract.
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
    /// Deposit funds for a given market
    Deposit {
        /// Market to apply funds to
        market: MarketId,
    },
    /// Withdraw funds from a given market
    Withdraw {
        /// The number of LP shares to remove
        amount: NonZero<LpToken>,
    },
    /// Perform a balancing operation on the given market
    Balance {
        /// Which markets to balance
        market: MarketId,
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
}

/// Queries that can be performed on the countertrade contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Get the current config
    ///
    /// Returns [Config]
    Config {},
    /// Check the balance of an address for all markets.
    ///
    /// Returns [BalanceResp]
    Balance {
        /// Address of the token holder
        address: RawAddr,
        /// Value from [BalanceResp::next_start_after]
        start_after: Option<MarketId>,
    },
    /// Check if the given market has any work to do
    ///
    /// Returns [HasWork]
    HasWork {
        /// Which market to check
        market: MarketId,
    },
}

/// Response from [QueryMsg::Balance]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct BalanceResp {
    /// Market balances in this batch
    pub markets: Vec<MarketBalance>,
    /// Next start_after value, if we have more balances
    pub next_start_after: Option<MarketId>,
}
/// Individual market response from [QueryMsg::Balance]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct MarketBalance {
    /// Market where a balance is held
    pub market: MarketId,
    /// Token for this market
    pub token: Token,
    /// Shares of the pool held by this LP
    pub shares: NonZero<LpToken>,
    /// Collateral equivalent of these shares
    pub collateral: NonZero<Collateral>,
    /// Size of the entire pool, in LP tokens
    pub pool_size: NonZero<LpToken>,
}

/// Either a native token or CW20 contract
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
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

/// Migration message, currently no fields needed
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}
