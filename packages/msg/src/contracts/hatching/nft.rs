use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_utils::Expiration;

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

/// Response for [QueryMsg::OwnerOf]
#[cw_serde]
pub struct OwnerOfResponse {
    /// Owner of the token
    pub owner: Addr,
    /// If set this address is approved to transfer/send the token as well
    pub approvals: Vec<Approval>,
}

/// Response for [QueryMsg::NftInfo]
#[cw_serde]
pub struct NftInfoResponse {
    /// Optional token_uri
    pub token_uri: Option<String>,
    /// You can add any custom metadata here when you extend cw721-base
    pub extension: Metadata,
}

/// copied/adapted from the cw721-base reference
#[cw_serde]
pub struct Approval {
    /// Account that can transfer/send the token
    pub spender: Addr,
    /// When the Approval expires (maybe Expiration::never)
    pub expires: Expiration,
}

/// NFT standard metadata
#[cw_serde]
#[derive(Default)]
pub struct Metadata {
    /// Unused by perps
    pub image: Option<String>,
    /// Unused by perps
    pub image_data: Option<String>,
    /// Unused by perps
    pub external_url: Option<String>,
    /// Unused by perps
    pub description: Option<String>,
    /// Unused by perps
    pub name: Option<String>,
    /// Characteristics of the position
    pub attributes: Option<Vec<Trait>>,
    /// Unused by perps
    pub background_color: Option<String>,
    /// Unused by perps
    pub animation_url: Option<String>,
    /// Unused by perps
    pub youtube_url: Option<String>,
}

/// NFT-standard traits, used to express information on the position
#[cw_serde]
#[derive(Eq, Default)]
pub struct Trait {
    /// Unused by pers
    pub display_type: Option<String>,
    /// The type of data contained in this trait.
    pub trait_type: String,
    /// The value for the given trait type.
    pub value: String,
}
