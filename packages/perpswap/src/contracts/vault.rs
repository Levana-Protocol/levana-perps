#![allow(missing_docs)]
use cosmwasm_std::{Addr, Uint128};

/// Message to instantiate the contract
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Denomination of the USDC token
    pub usdc_denom: String,

    /// Address of the factory (as string, validated later)
    pub factory_address: String,

    /// Address of the USDCLP contract (as string, validated later)
    pub usdclp_address: String,

    /// Governance address (as string, validated later)
    pub governance: String,

    /// Initial list of operators (as strings, validated later)
    pub initial_operators: Vec<String>,

    /// Initial allocation percentages to markets
    pub markets_allocation_bps: Vec<u16>,
}

/// Configuration structure for the vault
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    /// Denomination of the USDC token (e.g., "uusdc")
    pub usdc_denom: String,

    /// Token Factory denom for USDCLP, e.g., "factory/<vault_addr>/usdclp"
    pub usdclp_address: String,

    /// Address authorized for critical actions (like pausing the contract)
    pub governance: Addr,

    /// List of addresses authorized for specific operations
    pub operators: Vec<Addr>,

    /// Allocation percentages to markets in basis points (100 bps = 1%)
    pub markets_allocation_bps: Vec<u16>,

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
    RequestWithdrawal { amount: Uint128 },

    /// Redistribute excess funds to markets
    RedistributeFunds {},

    /// Collect yields from markets
    CollectYield { batch_limit: Option<u32> },

    /// Process a pending withdrawal
    ProcessWithdrawal {},

    /// Withdraw funds from a market
    WithdrawFromMarket { market: String, amount: Uint128 },

    /// Update the list of operators
    UpdateOperators {
        add: Vec<String>,
        remove: Vec<String>,
    },

    /// Pause the contract in an emergency
    EmergencyPause {},

    /// Resume contract operations
    ResumeOperations {},

    /// Update allocation percentages
    UpdateAllocations { new_allocations: Vec<u16> },
}

/// Query messages for the contract
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Query the USDC balance in the vault
    GetVaultBalance {},

    /// Query a user's pending withdrawal
    GetPendingWithdrawal { user: String },

    /// Query total assets (balance + allocations)
    GetTotalAssets {},

    /// Query market allocations
    GetMarketAllocations {
        start_after: Option<String>,
        limit: Option<u32>,
    },

    /// Query the vault's configuration
    GetConfig {},
}
