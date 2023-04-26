#![allow(missing_docs)]

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use shared::prelude::*;

use super::position_token::Metadata;
pub mod config;
pub mod entry;
pub mod events;
pub mod ibc;

#[cw_serde]
pub struct HatchStatus {
    pub nft_mint_completed: bool,
    pub lvn_grant_completed: bool,
    /// Only loaded if requested
    pub details: Option<HatchDetails>,
}

#[cw_serde]
pub struct HatchDetails {
    pub owner: Addr,
    pub hatch_time: Timestamp,
    pub eggs: Vec<NftHatchInfo>,
    pub dusts: Vec<NftHatchInfo>,
    // TODO: profile, etc (PERP-1154)
}

#[cw_serde]
pub struct NftHatchInfo {
    pub spirit_level: NumberGtZero,
    pub lvn: NumberGtZero,
    pub token_id: String,
    pub burn_kind: NftBurnKind,
    pub burn_metadata: Metadata,
    pub rarity: NftRarity,
}

#[cw_serde]
#[derive(Copy)]
pub enum NftBurnKind {
    /// Levana nested eggs
    Egg,
    /// Levana dust
    Dust,
}

#[cw_serde]
#[derive(Copy)]
pub enum NftRarity {
    Legendary,
    Ancient,
    Rare,
    Common,
}
