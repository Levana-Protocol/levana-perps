//! Vault contract
use super::cw20::Cw20ReceiveMsg;
use anyhow::{ensure, Result};
use cosmwasm_std::{Addr, Api, Uint128};
use serde::{Deserialize, Deserializer};
use std::{collections::HashMap, fmt};

/// Message to instantiate the contract
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Denomination of the USDC token
    pub usdc_denom: UsdcAssetInit,

    /// Governance address (as string, validated later)
    pub governance: String,

    /// Initial allocation percentages to markets
    pub markets_allocation_bps: HashMap<String, u16>,
}

/// Denomination of USDC token
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UsdcAsset {
    /// CW20 USDC
    CW20(Addr),
    /// Native USDC
    Native(String),
}
/// Input enum for user-provided asset specification
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum UsdcAssetInit {
    /// CW20 User Input
    CW20 {
        /// Address of the CW20 token
        address: String,
    },
    /// Native User Input
    Native {
        /// Denomination of the native token
        denom: String,
    },
}

impl fmt::Display for UsdcAsset {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UsdcAsset::CW20(addr) => write!(f, "{}", addr),
            UsdcAsset::Native(denom) => write!(f, "{}", denom),
        }
    }
}

impl UsdcAsset {
    /// Convert from user input with validation
    pub fn from_init(api: &dyn Api, init: UsdcAssetInit) -> Result<Self> {
        match init {
            UsdcAssetInit::CW20 { address } => {
                let addr = api.addr_validate(&address)?;
                Ok(UsdcAsset::CW20(addr))
            }
            UsdcAssetInit::Native { denom } => {
                ensure!(!denom.is_empty(), "Native denom cannot be empty");
                Ok(UsdcAsset::Native(denom))
            }
        }
    }
}

/// Configuration structure for the vault
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    /// Denomination of the USDC token
    pub usdc_denom: UsdcAsset,

    /// Address authorized for critical actions (like pausing the contract)
    pub governance: Addr,

    /// Allocation percentages to markets in basis points (100 bps = 1%)
    #[serde(deserialize_with = "deserialize_markets_allocation_bps")]
    pub markets_allocation_bps: HashMap<Addr, u16>,

    /// state::PAUSED
    pub paused: bool,
}

/// Executable messages for the contract
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Deposit USDC and receive USDCLP
    Deposit {},

    /// Receive CW20
    Receive(Cw20ReceiveMsg),

    /// Request withdrawal by burning USDCLP
    RequestWithdrawal {
        /// Amount to withdraw
        amount: Uint128,
    },

    /// Redistribute excess funds to markets
    RedistributeFunds {},

    /// Collect yields from markets
    CollectYield {},

    /// Process a pending withdrawal
    ProcessWithdrawal {},

    /// Withdraw funds from a market
    WithdrawFromMarket {
        /// From Market
        market: String,
        /// Amount
        amount: Uint128,
    },

    /// Pause the contract in an emergency
    EmergencyPause {},

    /// Resume contract operations
    ResumeOperations {},

    /// Update allocation percentages
    UpdateAllocations {
        /// New allocations for Markets
        #[serde(deserialize_with = "deserialize_markets_allocation_bps")]
        new_allocations: HashMap<Addr, u16>,
    },

    /// Add a new market to the vault
    AddMarket {
        /// Market address to add
        market: String,
    },
}

/// Query messages for the contract
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Query the USDC balance in the vault
    GetVaultBalance {},

    /// Query a user's pending withdrawal
    GetPendingWithdrawal {
        /// User to get pending withdrawal
        user: String,
    },

    /// Query total assets (balance + allocations)
    GetTotalAssets {},

    /// Query market allocations
    GetMarketAllocations {
        /// Start from market
        start_after: Option<String>,
    },

    /// Query the vault's configuration
    GetConfig {},
}

fn deserialize_markets_allocation_bps<'de, D>(
    deserializer: D,
) -> Result<HashMap<Addr, u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let string_map = HashMap::<String, u16>::deserialize(deserializer)?;
    let addr_map = string_map
        .into_iter()
        .map(|(k, v)| (Addr::unchecked(k), v))
        .collect();
    Ok(addr_map)
}
