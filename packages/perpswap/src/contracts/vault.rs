//! Vault contract
use cosmwasm_std::{Addr, Uint128};
use std::collections::HashMap;

/// Message to instantiate the contract
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Denomination of the USDC token
    pub usdc_denom: String,

    /// Governance address (as string, validated later)
    pub governance: String,

    /// Initial allocation percentages to markets
    pub markets_allocation_bps: HashMap<String, u16>,
}

/// Configuration structure for the vault
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    /// Denomination of the USDC token. Varies by chain (e.g., "uusdc" on Osmosis, "usdc" on Noble, ...)
    pub usdc_denom: String,

    /// Address authorized for critical actions (like pausing the contract)
    pub governance: Addr,

    /// Allocation percentages to markets in basis points (100 bps = 1%)
    pub markets_allocation_bps: HashMap<String, u16>,

    /// state::PAUSED
    pub paused: bool,
}

/// Executable messages for the contract
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Deposit USDC and receive USDCLP
    Deposit {},

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
        new_allocations: HashMap<String, u16>,
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
