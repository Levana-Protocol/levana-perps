use cosmwasm_std::{Addr, Uint128};
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketAllocation {
    /// Identifier of the market
    pub market_id: String,

    /// Initial USDC allocated to this market
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketAllocationsResponse {
    /// List of initial market allocations.
    pub allocations: Vec<MarketAllocation>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MarketQueryMsg {
    /// Retrieves the market's utilization
    GetUtilization {},
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MarketExecuteMsg {
    /// Deposits USDC into the market
    Deposit { amount: Uint128 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct WithdrawalRequest {
    /// Address of the user requesting withdrawal
    pub(crate) user: Addr,

    /// Amount of USDCLP to withdraw
    pub(crate) amount: Uint128,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub enum Cw20HookMsg {
    Deposit {},
}
