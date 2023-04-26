//! Messages for the perps position token contract.
//!
//! The position token is a proxy providing a CW721 (NFT) interface for all
//! positions within a single market.
pub mod entry;
pub mod events;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, BlockInfo};
use cw_utils::Expiration;

/// copied/adapted from the cw721-base reference
#[cw_serde]
pub struct Approval {
    /// Account that can transfer/send the token
    pub spender: Addr,
    /// When the Approval expires (maybe Expiration::never)
    pub expires: Expiration,
}

impl Approval {
    /// Is the given approval expired at the given block?
    pub fn is_expired(&self, block: &BlockInfo) -> bool {
        self.expires.is_expired(block)
    }
}

/// copied/adapted from the cw721-base reference
#[cw_serde]
pub struct FullTokenInfo {
    /// The owner of the newly minted NFT
    pub owner: Addr,
    /// Approvals are stored here, as we clear them all upon transfer and cannot accumulate much
    pub approvals: Vec<Approval>,

    /// metadata, as per spec
    pub extension: Metadata,
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

/// Cw721ReceiveMsg should be de/serialized under `Receive()` variant in a ExecuteMsg
#[cw_serde]
pub struct Cw721ReceiveMsg {
    /// Sender of the NFT
    pub sender: String,
    /// Position ID transferred
    pub token_id: String,
    /// Binary message for the receiving contract to execute.
    pub msg: Binary,
}
