use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::IbcOrder;
use shared::storage::RawAddr;

/// Instantiate message
#[cw_serde]
pub struct InstantiateMsg {
    /// The contract to pass execute messages
    pub contract: RawAddr,
    /// The expected channel version
    pub ibc_channel_version: String,
    /// The expected channel order
    pub ibc_channel_order: IbcOrder,
}

#[cw_serde]
pub enum ExecuteMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [super::config::Config]
    #[returns(super::config::Config)]
    Config {},
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}
