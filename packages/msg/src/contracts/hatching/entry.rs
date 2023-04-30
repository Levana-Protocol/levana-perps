use cosmwasm_schema::{cw_serde, QueryResponses};
use shared::storage::RawAddr;

use super::HatchStatus;

/// Instantiate message
#[cw_serde]
pub struct InstantiateMsg {
    pub burn_egg_contract: RawAddr,
    pub burn_dust_contract: RawAddr,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// hatch and get rewards
    Hatch {
        eggs: Vec<String>,
        dusts: Vec<String>,
    },

    /// Retry a hatch that's stuck
    RetryHatch { id: String },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [super::config::Config]
    #[returns(super::config::Config)]
    Config {},

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

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}
