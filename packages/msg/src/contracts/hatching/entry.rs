use cosmwasm_schema::{cw_serde, QueryResponses};
use shared::storage::RawAddr;

use super::{HatchStatus, NftHatchInfo, ProfileInfo, dragon_mint::DragonMintExtra};

/// Instantiate message
#[cw_serde]
pub struct InstantiateMsg {
    pub burn_egg_contract: RawAddr,
    pub burn_dust_contract: RawAddr,
    pub profile_contract: RawAddr,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// hatch and get rewards
    Hatch {
        /// Must be a valid address on the target minting network
        nft_mint_owner: String,
        /// Must be a valid address on the target lvn network
        lvn_grant_address: String,
        /// list of egg nft token ids to hatch
        eggs: Vec<String>,
        /// list of dust nft token ids to hatch
        dusts: Vec<String>,
        /// whether to also "hatch" the profile, i.e. drain the spirit level into lvn
        profile: bool,
    },

    /// Retry a hatch that's stuck
    RetryHatch { id: String },

    /// Admin-only: set the config
    SetBabyDragonExtras {
        /// list of baby dragon extras 
        extras: Vec<DragonMintExtra>,
    }
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [super::config::Config]
    #[returns(super::config::Config)]
    Config {},

    /// Query what a hatch would look like
    /// * returns [PotentialHatchInfo]
    #[returns(PotentialHatchInfo)]
    PotentialHatchInfo {
        /// The owner
        owner: RawAddr,
        /// list of egg nft token ids to hatch
        eggs: Vec<String>,
        /// list of dust nft token ids to hatch
        dusts: Vec<String>,
        /// whether to also "hatch" the profile, i.e. drain the spirit level into lvn
        profile: bool,
    },

    /// * returns [MaybeHatchStatusResp]
    #[returns(MaybeHatchStatusResp)]
    OldestHatchStatus { details: bool },

    /// * returns [MaybeHatchStatusResp]
    #[returns(MaybeHatchStatusResp)]
    HatchStatusByOwner { owner: RawAddr, details: bool },

    /// * returns [MaybeHatchStatusResp]
    #[returns(MaybeHatchStatusResp)]
    HatchStatusById { id: String, details: bool },
}

#[cw_serde]
pub struct MaybeHatchStatusResp {
    pub resp: Option<HatchStatusResp>,
}

#[cw_serde]
pub struct HatchStatusResp {
    pub id: String,
    pub status: HatchStatus,
}

impl From<(u64, HatchStatus)> for HatchStatusResp {
    fn from((id, status): (u64, HatchStatus)) -> Self {
        Self {
            id: id.to_string(),
            status,
        }
    }
}

#[cw_serde]
pub struct PotentialHatchInfo {
    pub eggs: Vec<NftHatchInfo>,
    pub dusts: Vec<NftHatchInfo>,
    pub profile: Option<ProfileInfo>,
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}
