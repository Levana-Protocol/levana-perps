#![allow(clippy::derive_partial_eq_without_eq)]
// Taken from levana-hatchery
// Except token-id which is a string instead of u64 newtype wrapper
use std::collections::HashSet;

use cw_utils::Expiration;
use msg::{contracts::hatching::NftRarity, prelude::NumberGtZero};
use serde::{Deserialize, Serialize};

pub use cosmwasm_std::Binary;

type TokenId = String;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Transfer is a base message to move a token to another account without triggering actions
    TransferNft {
        recipient: String,
        token_id: TokenId,
    },
    /// Send is a base message to transfer a token to a contract and trigger an action
    /// on the receiving contract.
    SendNft {
        contract: String,
        token_id: TokenId,
        msg: Binary,
    },
    /// Allows operator to transfer / send the token from the owner's account.
    /// If expiration is set, then this allowance has a time/height limit
    Approve {
        spender: String,
        token_id: TokenId,
        expires: Option<Expiration>,
    },
    /// Remove previously granted Approval
    Revoke { spender: String, token_id: TokenId },
    /// Allows operator to transfer / send any token from the owner's account.
    /// If expiration is set, then this allowance has a time/height limit
    ApproveAll {
        operator: String,
        expires: Option<Expiration>,
    },
    /// Remove previously granted ApproveAll permission
    RevokeAll { operator: String },

    /// Mint a new NFT, can only be called by the contract minter
    Mint(Box<MintMsg>),

    /// Add Minters, can only be called by the contract minters
    AddMinters { minters: HashSet<String> },

    /// Remove Minters, can only be called by the contract minters
    RemoveMinters { minters: HashSet<String> },

    /// Burn an NFT the sender has access to
    Burn { token_id: TokenId },

    /// Update the metadata on the given token ID, can only be called by the contract minter
    Update(Box<UpdateMsg>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MintMsg {
    /// Unique ID of the NFT
    pub token_id: TokenId,
    /// The owner of the newly minted NFT
    pub owner: String,
    /// Universal resource identifier for this NFT
    /// Should point to a JSON file that conforms to the ERC721
    /// Metadata JSON Schema
    pub token_uri: Option<String>,
    /// Any custom extension used by this contract
    pub extension: Metadata,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UpdateMsg {
    /// Unique ID of the NFT
    pub token_id: TokenId,
    /// New metadata
    pub extension: Metadata,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Metadata {
    pub image: String,
    pub image_data: Option<String>,
    pub external_url: Option<String>,
    pub description: String,
    pub name: String,
    pub attributes: Vec<Trait>,
    pub background_color: Option<String>,
    pub animation_url: Option<String>,
    pub youtube_url: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Trait {
    pub display_type: Option<String>,
    pub trait_type: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return the owner of the given token, error if token does not exist
    /// Return type: OwnerOfResponse
    OwnerOf {
        token_id: TokenId,
        /// unset or false will filter out expired approvals, you must set to true to see them
        include_expired: Option<bool>,
    },
    /// List all operators that can access all of the owner's tokens
    /// Return type: `ApprovedForAllResponse`
    ApprovedForAll {
        owner: String,
        /// unset or false will filter out expired items, you must set to true to see them
        include_expired: Option<bool>,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Total number of tokens issued
    NumTokens {},

    /// With MetaData Extension.
    /// Returns top-level metadata about the contract: [ContractInfoResponse]
    ContractInfo {},
    /// With MetaData Extension.
    /// Returns metadata about one particular token, based on *ERC721 Metadata JSON Schema*
    /// but directly from the contract: `NftInfoResponse`
    NftInfo { token_id: TokenId },
    /// With MetaData Extension.
    /// Returns the result of both `NftInfo` and `OwnerOf` as one query as an optimization
    /// for clients: `AllNftInfo`
    AllNftInfo {
        token_id: TokenId,
        /// unset or false will filter out expired approvals, you must set to true to see them
        include_expired: Option<bool>,
    },

    /// With Enumerable extension.
    /// Returns all tokens owned by the given address, [] if unset.
    /// Return type: TokensResponse.
    Tokens {
        owner: String,
        start_after: Option<TokenId>,
        limit: Option<u32>,
    },
    /// With Enumerable extension.
    /// Requires pagination. Lists all token_ids controlled by the contract.
    /// Return type: TokensResponse.
    AllTokens {
        start_after: Option<TokenId>,
        limit: Option<u32>,
    },

    /// Return the minter
    Minter {},

    /// Return the highest used token ID
    HighestTokenId {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OwnerOfResponse {
    /// Owner of the token
    pub owner: String,
    /// If set this address is approved to transfer/send the token as well
    pub approvals: Vec<Approval>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Approval {
    /// Account that can transfer/send the token
    pub spender: String,
    /// When the Approval expires (maybe Expiration::never)
    pub expires: Expiration,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ApprovedForAllResponse {
    pub operators: Vec<Approval>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NumTokensResponse {
    pub count: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct HighestTokenIdResponse {
    pub highest_token_id: Option<TokenId>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ContractInfoResponse {
    pub name: String,
    pub symbol: String,
    // Loop specific fields
    /// List of royalty rate in basis points. 100 == 1%. Must be same length as `royalty_addrs`.
    pub royalty_bps: Option<Vec<u32>>,
    /// List of addresses that receive royalties
    pub royalty_addrs: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NftInfoResponse {
    /// Universal resource identifier for this NFT
    /// Should point to a JSON file that conforms to the ERC721
    /// Metadata JSON Schema
    pub token_uri: Option<String>,
    /// You can add any custom metadata here when you extend cw721-base
    pub extension: Metadata,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AllNftInfoResponse {
    /// Who can transfer the token
    pub access: OwnerOfResponse,
    /// Data on the token itself,
    pub info: NftInfoResponse,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TokensResponse {
    /// Contains all token_ids in lexicographical ordering
    /// If there are more than `limit`, use `start_from` in future queries
    /// to achieve pagination.
    pub tokens: Vec<TokenId>,
}

/// Shows who can mint these tokens
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct MinterResponse {
    pub minter: HashSet<String>,
}

impl Metadata {
    pub fn new_egg(
        token_id: String,
        spirit_level: NumberGtZero,
        rarity: NftRarity,
        dragon_type: String,
    ) -> Self {
        let mut m: Self = serde_json::from_str(EGG_META).unwrap();

        // in theory we'd want to show a different egg based on rarity etc.
        // but it's okay, this is just for testing, they can all look the same
        m.image = "ipfs://QmecraVcH6N9Niai53m16zE2bo2rmxsu8ukNi25UvSFuZF".to_string();

        m.name = format!("Levana Dragons: Rare Nested Egg #{}", token_id);

        m.attributes.push(Trait {
            display_type: None,
            trait_type: "Spirit Level".to_string(),
            value: spirit_level.to_string(),
        });

        m.attributes.push(Trait {
            display_type: None,
            trait_type: "Rarity".to_string(),
            value: match rarity {
                NftRarity::Common => "Common".to_string(),
                NftRarity::Rare => "Rare".to_string(),
                NftRarity::Ancient => "Ancient".to_string(),
                NftRarity::Legendary => "Legendary".to_string(),
            },
        });

        m.attributes.push(Trait {
            display_type: None,
            trait_type: "Dragon Type".to_string(),
            value: dragon_type,
        });

        m
    }
    pub fn new_dust(spirit_level: NumberGtZero, rarity: NftRarity) -> Self {
        let mut m: Self = serde_json::from_str(DUST_META).unwrap();

        // in theory we'd want to show a different dust based on rarity etc.
        // but it's okay, this is just for testing, they can all look the same
        m.image = "ipfs://QmPYGyUarK7L4oUdB7esxnTFHUhexfnxxzFgxRZSgQVsKA".to_string();

        m.attributes.push(Trait {
            display_type: None,
            trait_type: "Spirit Level".to_string(),
            value: spirit_level.to_string(),
        });

        m.attributes.push(Trait {
            display_type: None,
            trait_type: "Rarity".to_string(),
            value: match rarity {
                NftRarity::Common => "Common".to_string(),
                NftRarity::Rare => "Rare".to_string(),
                NftRarity::Ancient => "Ancient".to_string(),
                NftRarity::Legendary => "Legendary".to_string(),
            },
        });

        m
    }
}
static EGG_META: &str = r#"{
    "image":"ipfs://replaceme",
    "image_data":null,
    "external_url":null,
    "description":"Evolutionary Rare Nested Egg NFT, stage 3 of the Levana Dragons adventure.",
    "name":"replaceme",
    "attributes":[
        {
            "display_type":null,
            "trait_type":"Stage",
            "value":"Nested Egg"
        },
        {
            "display_type":null,
            "trait_type":"Origin",
            "value":"Southern hemisphere subterranean caves"
        },
        {
            "display_type":null,
            "trait_type":"Essence",
            "value":"Electric"
        },
        {
            "display_type":null,
            "trait_type":"Rare Composition",
            "value":"Nitrogen"
        },
        {
            "display_type":null,
            "trait_type":"Common Composition",
            "value":"Sodium"
        },
        {
            "display_type":null,
            "trait_type":"Family",
            "value":"Oquania"
        },
        {
            "display_type":null,
            "trait_type":"Genus",
            "value":"Chaos"
        },
        {
            "display_type":null,
            "trait_type":"Affecting Moon",
            "value":"Sao"
        },
        {
            "display_type":null,
            "trait_type":"Lucky Number",
            "value":"1"
        },
        {
            "display_type":null,
            "trait_type":"Constellation",
            "value":"Cerberus"
        },
        {
            "display_type":null,
            "trait_type":"Nesting Date",
            "value":"2472-02-01"
        }
    ],
    "background_color":null,
    "animation_url":null,
    "youtube_url":null
}"#;

static DUST_META: &str = r#"
{

    "image":"ipfs://replaceme",
    "image_data":null,
    "external_url":null,
    "description":"Evolutionary Rare Meteor Dust NFT, stage 2 of the Levana Dragons adventure.",
    "name":"Levana Dragons: Rare Meteor Dust",
    "attributes":[
        {
            "display_type":null,
            "trait_type":"Type",
            "value":"Meteor Dust"
        },
        {
            "display_type":null,
            "trait_type":"Dust Volume",
            "value":"Quarter"
        },
        {
            "display_type":null,
            "trait_type":"Essence",
            "value":"Psychic"
        },
        {
            "display_type":null,
            "trait_type":"Rare Gem",
            "value":"Cinnabar"
        },
        {
            "display_type":null,
            "trait_type":"Common Gem",
            "value":"Azurite"
        },
        {
            "display_type":null,
            "trait_type":"Rare Composition",
            "value":"Sulfur"
        },
        {
            "display_type":null,
            "trait_type":"Common Composition",
            "value":"Silicon"
        }
    ],
    "background_color":null,
    "animation_url":null,
    "youtube_url":null
 }
"#;
