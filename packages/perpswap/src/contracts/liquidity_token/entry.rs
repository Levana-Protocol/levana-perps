//! Entrypoint messages for liquidity token proxy
use super::LiquidityTokenKind;
use crate::prelude::*;
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Binary, Uint128};
use cw_utils::Expiration;

/// Instantiate message for liquidity token proxy
#[cw_serde]
pub struct InstantiateMsg {
    /// The factory address
    pub factory: RawAddr,
    /// Unique market identifier, also used for `symbol` in ContractInfo response
    pub market_id: MarketId,
    /// The liquidity token kind
    pub kind: LiquidityTokenKind,
}

/// Execute message for liquidity token proxy
#[cw_serde]
pub enum ExecuteMsg {
    /************** Cw20 spec *******************/
    /// Transfer is a base message to move tokens to another account without triggering actions
    Transfer {
        /// Recipient of the funds
        recipient: RawAddr,
        /// Amount to transfer
        amount: Uint128,
    },
    /// Send is a base message to transfer tokens to a contract and trigger an action
    /// on the receiving contract.
    Send {
        /// Contract to receive the funds
        contract: RawAddr,
        /// Amount to send
        amount: Uint128,
        /// Message to execute on the receiving contract
        msg: Binary,
    },
    /// Allows spender to access an additional amount tokens
    /// from the owner's (env.sender) account. If expires is Some(), overwrites current allowance
    /// expiration with this one.
    IncreaseAllowance {
        /// Who is allowed to spend
        spender: RawAddr,
        /// Amount they can spend
        amount: Uint128,
        /// When the allowance expires
        expires: Option<Expiration>,
    },
    /// Lowers the spender's access of tokens
    /// from the owner's (env.sender) account by amount. If expires is Some(), overwrites current
    /// allowance expiration with this one.
    DecreaseAllowance {
        /// Whose spending to reduced
        spender: RawAddr,
        /// Amount to reduce by
        amount: Uint128,
        /// When the allowance should expire
        expires: Option<Expiration>,
    },
    /// Transfers amount tokens from owner -> recipient
    /// if `env.sender` has sufficient pre-approval.
    TransferFrom {
        /// Owner of the tokens being transferred
        owner: RawAddr,
        /// Recipient of the tokens
        recipient: RawAddr,
        /// Amount to send
        amount: Uint128,
    },
    /// Sends amount tokens from owner -> contract
    /// if `env.sender` has sufficient pre-approval.
    SendFrom {
        /// Owner of the tokens being transferred
        owner: RawAddr,
        /// Contract to receive the funds
        contract: RawAddr,
        /// Amount to send
        amount: Uint128,
        /// Message to execute on the receiving contract
        msg: Binary,
    },
}

/// Query message for liquidity token proxy
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /************** Cw20 spec *******************/
    /// * returns [crate::contracts::cw20::entry::BalanceResponse]
    ///
    /// The current balance of the given address, 0 if unset.
    #[returns(crate::contracts::cw20::entry::BalanceResponse)]
    Balance {
        /// Address whose balance to check
        address: RawAddr,
    },

    /// * returns [crate::contracts::cw20::entry::TokenInfoResponse]
    ///
    /// Returns metadata on the contract - name, decimals, supply, etc.
    #[returns(crate::contracts::cw20::entry::TokenInfoResponse)]
    TokenInfo {},

    /// * returns [crate::contracts::cw20::entry::AllowanceResponse]
    ///
    /// Returns how much spender can use from owner account, 0 if unset.
    #[returns(crate::contracts::cw20::entry::AllowanceResponse)]
    Allowance {
        /// Owner of tokens
        owner: RawAddr,
        /// Who is allowed to spend them
        spender: RawAddr,
    },

    /// * returns [crate::contracts::cw20::entry::AllAllowancesResponse]
    ///
    /// Returns all allowances this owner has approved. Supports pagination.
    #[returns(crate::contracts::cw20::entry::AllAllowancesResponse)]
    AllAllowances {
        /// Owner of tokens
        owner: RawAddr,
        /// Last spender we saw
        start_after: Option<RawAddr>,
        /// How many spenders to iterate on
        limit: Option<u32>,
    },

    /// * returns [crate::contracts::cw20::entry::AllSpenderAllowancesResponse]
    ///
    /// Returns all allowances this spender has been granted. Supports pagination.
    #[returns(crate::contracts::cw20::entry::AllSpenderAllowancesResponse)]
    AllSpenderAllowances {
        /// Spender address
        spender: RawAddr,
        /// Last owner we saw
        start_after: Option<RawAddr>,
        /// How many owners to iterate on
        limit: Option<u32>,
    },

    /// * returns [crate::contracts::cw20::entry::AllAccountsResponse]
    ///
    /// Returns all accounts that have balances. Supports pagination.
    #[returns(crate::contracts::cw20::entry::AllAccountsResponse)]
    AllAccounts {
        /// Last owner we saw
        start_after: Option<RawAddr>,
        /// How many owners to iterate on
        limit: Option<u32>,
    },

    /// * returns [crate::contracts::cw20::entry::MarketingInfoResponse]
    ///
    /// Returns more metadata on the contract to display in the client:
    /// - description, logo, project url, etc.
    #[returns(crate::contracts::cw20::entry::MarketingInfoResponse)]
    MarketingInfo {},

    /************** Proprietary *******************/
    /// * returns [cw2::ContractVersion]
    #[returns(cw2::ContractVersion)]
    Version {},

    /// * returns [LiquidityTokenKind]
    #[returns(LiquidityTokenKind)]
    Kind {},
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}
