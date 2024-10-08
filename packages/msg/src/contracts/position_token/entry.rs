//! Entrypoint messages for position token proxy
use std::num::ParseIntError;

use crate::contracts::market::position::PositionId;

use super::{Approval, Metadata};
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Binary};
use cw_utils::Expiration;
use perpswap::prelude::*;

/// Instantiate a new position token proxy contract
#[cw_serde]
pub struct InstantiateMsg {
    /// The factory address
    pub factory: RawAddr,
    /// Unique market identifier, also used for `symbol` in ContractInfo response
    pub market_id: MarketId,
}

/// Execute messages for a position token proxy
///
/// Matches the CW721 standard.
#[cw_serde]
pub enum ExecuteMsg {
    /// Transfer is a base message to move a token to another account without triggering actions
    TransferNft {
        /// Recipient of the NFT (position)
        recipient: RawAddr,
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
    },
    /// Send is a base message to transfer a token to a contract and trigger an action
    /// on the receiving contract.
    SendNft {
        /// Contract to receive the position
        contract: RawAddr,
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
        /// Message to execute on the contract
        msg: Binary,
    },
    /// Allows operator to transfer / send the token from the owner's account.
    /// If expiration is set, then this allowance has a time/height limit
    Approve {
        /// Address that is allowed to spend the NFT
        spender: RawAddr,
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
        /// When the approval expires
        expires: Option<Expiration>,
    },
    /// Remove previously granted Approval
    Revoke {
        /// Address that is no longer allowed to spend the NFT
        spender: RawAddr,
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
    },
    /// Allows operator to transfer / send any token from the owner's account.
    /// If expiration is set, then this allowance has a time/height limit
    ApproveAll {
        /// Address that is allowed to spend all NFTs by the sending wallet
        operator: RawAddr,
        /// When the approval expires
        expires: Option<Expiration>,
    },
    /// Remove previously granted ApproveAll permission
    RevokeAll {
        /// Address that is no longer allowed to spend all NFTs
        operator: RawAddr,
    },
}

impl ExecuteMsg {
    /// Get the position ID from this message, if there is one.
    pub fn get_position_id(&self) -> Result<Option<PositionId>, ParseIntError> {
        match self {
            ExecuteMsg::TransferNft {
                recipient: _,
                token_id,
            }
            | ExecuteMsg::SendNft {
                contract: _,
                token_id,
                msg: _,
            }
            | ExecuteMsg::Approve {
                spender: _,
                token_id,
                expires: _,
            }
            | ExecuteMsg::Revoke {
                spender: _,
                token_id,
            } => token_id.parse().map(Some),
            ExecuteMsg::ApproveAll {
                operator: _,
                expires: _,
            }
            | ExecuteMsg::RevokeAll { operator: _ } => Ok(None),
        }
    }
}

/// Query messages for a position token proxy
///
/// Matches the CW721 standard.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    //*************** CW-721 SPEC *********************//
    /// * returns [OwnerOfResponse]
    ///
    /// Return the owner of the given token, error if token does not exist
    #[returns(OwnerOfResponse)]
    OwnerOf {
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
        /// unset or false will filter out expired approvals, you must set to true to see them
        include_expired: Option<bool>,
    },

    /// * returns [ApprovalResponse]
    ///
    /// Return operator that can access all of the owner's tokens.
    #[returns(ApprovalResponse)]
    Approval {
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
        /// Spender
        spender: RawAddr,
        /// Should we include expired approvals?
        include_expired: Option<bool>,
    },

    /// * returns [ApprovalsResponse]
    ///
    /// Return approvals that a token has
    #[returns(ApprovalsResponse)]
    Approvals {
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
        /// Should we include expired approvals?
        include_expired: Option<bool>,
    },

    /// * returns [OperatorsResponse]
    ///
    /// List all operators that can access all of the owner's tokens
    #[returns(OperatorsResponse)]
    AllOperators {
        /// Position ID, represented as a `String` to match the NFT spec
        owner: RawAddr,
        /// unset or false will filter out expired items, you must set to true to see them
        include_expired: Option<bool>,
        /// Last operator seen
        start_after: Option<String>,
        /// How many operators to return
        limit: Option<u32>,
    },

    /// * returns [NumTokensResponse]
    ///
    /// Total number of tokens issued
    #[returns(NumTokensResponse)]
    NumTokens {},

    /// * returns [NftContractInfo]
    ///
    /// Returns top-level metadata about the contract: `ContractInfoResponse`
    #[returns(NftContractInfo)]
    ContractInfo {},

    /// * returns [NftInfoResponse]
    ///
    /// Returns metadata for a given token/position
    /// the format is based on the *ERC721 Metadata JSON Schema*
    /// but directly from the contract: `NftInfoResponse`
    #[returns(NftInfoResponse)]
    NftInfo {
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
    },

    /// * returns [AllNftInfoResponse]
    ///
    /// Returns the result of both `NftInfo` and `OwnerOf` as one query as an optimization
    /// for clients: `AllNftInfo`
    #[returns(AllNftInfoResponse)]
    AllNftInfo {
        /// Position ID, represented as a `String` to match the NFT spec
        token_id: String,
        /// unset or false will filter out expired approvals, you must set to true to see them
        include_expired: Option<bool>,
    },

    /// * returns [TokensResponse]
    ///
    /// Returns all tokens owned by the given address, [] if unset.
    #[returns(TokensResponse)]
    Tokens {
        /// Owner to enumerate over
        owner: RawAddr,
        /// Last position ID seen
        start_after: Option<String>,
        /// Number of positions to return
        limit: Option<u32>,
    },

    /// * returns [TokensResponse]
    ///
    /// Requires pagination. Lists all token_ids controlled by the contract.
    #[returns(TokensResponse)]
    AllTokens {
        /// Last position ID seen
        start_after: Option<String>,
        /// Number of positions to return
        limit: Option<u32>,
    },

    //*************** PROPRIETARY *********************//
    /// * returns [cw2::ContractVersion]
    #[returns(cw2::ContractVersion)]
    Version {},
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}

/// Response for [QueryMsg::OwnerOf]
#[cw_serde]
pub struct OwnerOfResponse {
    /// Owner of the token
    pub owner: Addr,
    /// If set this address is approved to transfer/send the token as well
    pub approvals: Vec<Approval>,
}

/// Response for [QueryMsg::Approval]
#[cw_serde]
pub struct ApprovalResponse {
    /// Approval information
    pub approval: Approval,
}

/// Response for [QueryMsg::Approvals]
#[cw_serde]
pub struct ApprovalsResponse {
    /// Approval information
    pub approvals: Vec<Approval>,
}

/// Response for [QueryMsg::Operators]
#[cw_serde]
pub struct OperatorsResponse {
    /// Operator approval information
    pub operators: Vec<Approval>,
}

/// Response for [QueryMsg::NumTokens]
#[cw_serde]
pub struct NumTokensResponse {
    /// Total number of tokens in the protocol
    pub count: u64,
}

/// Response for [QueryMsg::ContractInfo]
#[cw_serde]
pub struct NftContractInfo {
    /// Name of this contract
    pub name: String,
    /// Ticker symbol for this contract
    pub symbol: String,
}

/// Response for [QueryMsg::NftInfo]
#[cw_serde]
pub struct NftInfoResponse {
    /// You can add any custom metadata here when you extend cw721-base
    pub extension: Metadata,
}

/// Response for [QueryMsg::AllNftInfo]
#[cw_serde]
pub struct AllNftInfoResponse {
    /// Who can transfer the token
    pub access: OwnerOfResponse,
    /// Data on the token itself,
    pub info: NftInfoResponse,
}

/// Response for [QueryMsg::Tokens]
#[cw_serde]
pub struct TokensResponse {
    /// Contains all token_ids in lexicographical ordering
    /// If there are more than `limit`, use `start_from` in future queries
    /// to achieve pagination.
    pub tokens: Vec<String>,
}
