use cosmwasm_std::Uint128;
use perpswap::time::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VaultBalanceResponse {
    /// USDC held directly by the contract (native balance).
    pub vault_balance: Uint128,

    /// Initial USDC allocated to markets, not the current LP value.
    pub allocated_amount: Uint128,

    /// Total pending withdrawals.
    pub pending_withdrawals: Uint128,

    /// Sum of vault_balance + allocated_amount (initial allocation total).
    pub total_allocated: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PendingWithdrawalResponse {
    /// Pending withdrawal amount in USDC
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TotalAssetsResponse {
    /// Sum of vault balance and initial market allocations
    pub total_assets: Uint128,
}

/// Represents a single market allocation.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketAllocation {
    /// Identifier of the market
    pub market_id: String,

    /// Initial USDC allocated to this market
    pub amount: Uint128,
}

/// Response for GetMarketAllocations query.
///
/// Contains a list of market allocations, structured as an object to allow
/// future expansion (e.g., pagination metadata, status).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketAllocationsResponse {
    /// List of initial market allocations.
    pub allocations: Vec<MarketAllocation>,
}

/// Response for IsPaused query.
///
/// Indicates whether the vault is paused, structured as an object to allow
/// future expansion (e.g., reason, timestamp).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IsPausedResponse {
    /// True if the vault is paused, false otherwise
    pub is_paused: bool,
}

/// Response for GetOperators query.
///
/// Contains the list of operators authorized in the vault, structured as an object
/// to allow future expansion (e.g., count, metadata).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OperatorsResponse {
    /// List of operator addresses
    pub operators: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GetUtilizationResponse {
    pub utilization: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MarketQueryMsg {
    /// Retrieves the market's utilization
    GetUtilization {},
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MarketExecuteMsg {
    /// Deposits USDC into the market
    Deposit { amount: Uint128 },
}

// Define a structure for withdrawal requests to ensure FIFO processing
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct WithdrawalRequest {
    /// Address of the user requesting withdrawal
    pub(crate) user: String,

    /// Amount of USDCLP to withdraw
    pub(crate) amount: Uint128,

    /// Timestamp to enforce FIFO order
    pub(crate) timestamp: Timestamp,
}
