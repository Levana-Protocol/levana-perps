//! Countertrade contract

use std::fmt::Display;

use crate::{
    price::PriceBaseInQuote,
    storage::{
        Collateral, DirectionToBase, LeverageToBase, LpToken, MarketId, NonZero, RawAddr,
        TakeProfitTrader,
    },
};
use cosmwasm_std::{Addr, Binary, Decimal256, Uint128};

use super::market::{
    deferred_execution::DeferredExecId,
    position::{PositionId, PositionQueryResponse},
};

/// Message for instantiating a new countertrade contract.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Market contract we're countertrading on
    pub market: RawAddr,
    /// Administrator of the contract
    pub admin: RawAddr,
    /// Initial configuration values
    pub config: ConfigUpdate,
}

/// Full configuration
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
/// Updates to configuration values.
pub struct Config {
    /// Administrator of the contract
    pub admin: Addr,
    /// Pending administrator, ready to be accepted, if any.
    pub pending_admin: Option<Addr>,
    /// Market address that we are allowed to open positions on
    pub market: Addr,
    /// Minimum funding rate for popular side
    pub min_funding: Decimal256,
    /// Target funding rate for popular side
    pub target_funding: Decimal256,
    /// Maximum funding rate for popular side
    pub max_funding: Decimal256,
    /// Allowed iterations to compute delta notional
    pub iterations: u8,
    /// Factor used to compute take profit price
    pub take_profit_factor: Decimal256,
    /// Factor used to compute stop loss price
    pub stop_loss_factor: Decimal256,
    /// Maximum leverage value we'll use
    ///
    /// If a market has lower max leverage, we use that instead
    pub max_leverage: LeverageToBase,
}

impl Config {
    /// Check validity of config values
    pub fn check(&self) -> anyhow::Result<()> {
        if self.min_funding >= self.target_funding {
            Err(anyhow::anyhow!(
                "Minimum funding must be strictly less than target"
            ))
        } else if self.target_funding >= self.max_funding {
            Err(anyhow::anyhow!(
                "Target funding must be strictly less than max"
            ))
        } else if self.max_leverage.into_decimal256() < Decimal256::from_ratio(2u32, 1u32) {
            Err(anyhow::anyhow!("Max leverage must be at least 2"))
        } else {
            Ok(())
        }
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
    pub iterations: Option<u8>,
    pub take_profit_factor: Option<Decimal256>,
    pub stop_loss_factor: Option<Decimal256>,
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
    Deposit {},
    /// Withdraw funds from a given market
    Withdraw {
        /// The number of LP shares to remove
        amount: NonZero<LpToken>,
    },
    /// Perform a balancing operation on the given market
    DoWork {},
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
    /// Returns [MarketBalance]
    Balance {
        /// Address of the token holder
        address: RawAddr,
    },
    /// Check the status of a single market
    ///
    /// Returns [MarketStatus]
    Status {},
    /// Check if the given contract has any work to do
    ///
    /// Returns [HasWorkResp]
    HasWork {},
}

/// Individual market response from [QueryMsg::Balance]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MarketBalance {
    /// Market where a balance is held
    pub market: MarketId,
    /// Token for this market
    pub token: crate::token::Token,
    /// Shares of the pool held by this specific wallet.
    pub shares: LpToken,
    /// Collateral equivalent of these shares
    pub collateral: Collateral,
    /// Size of the entire pool, in LP tokens
    pub pool_size: LpToken,
}

/// Either a native token or CW20 contract
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Token {
    /// Native coin and its denom
    Native(String),
    /// CW20 contract and its address
    Cw20(Addr),
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

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Token::Native(denom) => f.write_str(denom),
            Token::Cw20(addr) => f.write_str(addr.as_str()),
        }
    }
}

/// Status of a single market
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct MarketStatus {
    /// Which market
    pub id: MarketId,
    /// Collateral held inside the contract
    ///
    /// Does not include active collateral of a position
    pub collateral: Collateral,
    /// Number of outstanding shares
    pub shares: LpToken,
    /// Our open position, if we have exactly one
    pub position: Option<PositionQueryResponse>,
    /// Do we have too many open positions?
    pub too_many_positions: bool,
}

/// Whether or not there is work available.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HasWorkResp {
    /// No work is available
    NoWork {},
    /// There is work available to be done
    Work {
        /// A description of the work, for display and testing purposes.
        desc: WorkDescription,
    },
}

/// Work to be performed for a specific market.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkDescription {
    /// Open a new position
    OpenPosition {
        /// Direction of the new position
        direction: DirectionToBase,
        /// Leverage
        leverage: LeverageToBase,
        /// Amount of deposit collateral
        collateral: NonZero<Collateral>,
        /// Take profit value
        take_profit: TakeProfitTrader,
        /// Stop loss price of new position
        stop_loss_override: Option<PriceBaseInQuote>,
    },
    /// Close an unnecessary position
    ClosePosition {
        /// Position to be closed
        pos_id: PositionId,
    },
    /// All collateral exhausted, reset shares to 0
    ResetShares,
    /// Deferred execution completed, we can continue our processing
    ClearDeferredExec {
        /// ID to be cleared
        id: DeferredExecId,
    },
    /// Add collateral to a position, causing notional size to increase
    UpdatePositionAddCollateralImpactSize {
        /// ID of position to update
        pos_id: PositionId,
        /// Amount of funds to add to the position
        amount: NonZero<Collateral>,
    },
    /// Remove collateral from a position, causing notional size to decrease
    UpdatePositionRemoveCollateralImpactSize {
        /// ID of position to update
        pos_id: PositionId,
        /// Amount of funds to remove from the position
        amount: NonZero<Collateral>,
        /// Crank fee to be paid
        crank_fee: Collateral,
    },
}

impl WorkDescription {
    /// Is it closed position ?
    pub fn is_close_position(&self) -> bool {
        match self {
            WorkDescription::OpenPosition { .. } => false,
            WorkDescription::ClosePosition { .. } => true,
            WorkDescription::ResetShares => false,
            WorkDescription::ClearDeferredExec { .. } => false,
            WorkDescription::UpdatePositionAddCollateralImpactSize { .. } => false,
            WorkDescription::UpdatePositionRemoveCollateralImpactSize { .. } => false,
        }
    }

    /// Is it collect closed position ?
    pub fn is_collect_closed_position(&self) -> bool {
        match self {
            WorkDescription::OpenPosition { .. } => false,
            WorkDescription::ClosePosition { .. } => false,
            WorkDescription::ResetShares => false,
            WorkDescription::ClearDeferredExec { .. } => false,
            WorkDescription::UpdatePositionAddCollateralImpactSize { .. } => false,
            WorkDescription::UpdatePositionRemoveCollateralImpactSize { .. } => false,
        }
    }

    /// Is it update position ?
    pub fn is_update_position(&self) -> bool {
        match self {
            WorkDescription::OpenPosition { .. } => false,
            WorkDescription::ClosePosition { .. } => false,
            WorkDescription::ResetShares => false,
            WorkDescription::ClearDeferredExec { .. } => false,
            WorkDescription::UpdatePositionAddCollateralImpactSize { .. } => true,
            WorkDescription::UpdatePositionRemoveCollateralImpactSize { .. } => true,
        }
    }

    /// Is it open position ?
    pub fn is_open_position(&self) -> bool {
        match self {
            WorkDescription::OpenPosition { .. } => true,
            WorkDescription::ClosePosition { .. } => false,
            WorkDescription::ResetShares => false,
            WorkDescription::ClearDeferredExec { .. } => false,
            WorkDescription::UpdatePositionAddCollateralImpactSize { .. } => false,
            WorkDescription::UpdatePositionRemoveCollateralImpactSize { .. } => false,
        }
    }

    /// Is it open position ?
    pub fn is_handle_deferred_exec(&self) -> bool {
        match self {
            WorkDescription::OpenPosition { .. } => false,
            WorkDescription::ClosePosition { .. } => false,
            WorkDescription::ResetShares => false,
            WorkDescription::ClearDeferredExec { .. } => true,
            WorkDescription::UpdatePositionAddCollateralImpactSize { .. } => false,
            WorkDescription::UpdatePositionRemoveCollateralImpactSize { .. } => false,
        }
    }
}

impl std::fmt::Display for WorkDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkDescription::OpenPosition {
                direction,
                leverage,
                collateral,
                ..
            } => write!(
                f,
                "Open {direction:?} position with leverage {leverage} and collateral {collateral}"
            ),
            WorkDescription::ClosePosition { pos_id } => write!(f, "Close Position {pos_id}"),
            WorkDescription::ResetShares => write!(f, "Reset Shares"),
            WorkDescription::ClearDeferredExec { id, .. } => {
                write!(f, "Handle Deferred Exec Id of {id}")
            }
            WorkDescription::UpdatePositionAddCollateralImpactSize { pos_id, amount } => {
                write!(
                    f,
                    "Add {amount} Collateral to Position Id of {pos_id} impacting size"
                )
            }
            WorkDescription::UpdatePositionRemoveCollateralImpactSize {
                pos_id, amount, ..
            } => write!(
                f,
                "Remove {amount} Collateral to Position Id of {pos_id} impacting size"
            ),
        }
    }
}

/// Migration message, currently no fields needed
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}
