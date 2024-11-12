use perpswap::prelude::*;

pub(crate) const ADMINS: Map<&Addr, ()> = Map::new("admins");

pub(crate) const CODE_BY_HASH: Map<&str, u64> = Map::new("code-by-hash");
pub(crate) const CODE_BY_ID: Map<u64, CodeIdInfo> = Map::new("code-by-id");

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct CodeIdInfo {
    pub(crate) contract_type: String,
    pub(crate) code_id: u64,
    pub(crate) hash: String,
    pub(crate) tracked_at: Timestamp,
    pub(crate) gitrev: Option<String>,
}

/// Contracts by: family, then contract type, then sequence number
pub(crate) const CONTRACT_BY_FAMILY: Map<((&str, &str), u32), Addr> =
    Map::new("contract-by-family");

pub(crate) const CONTRACT_BY_ADDR: Map<&Addr, ContractInfo> = Map::new("contract-by-addr");

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct ContractInfo {
    pub(crate) original_code_id: u64,
    pub(crate) original_tracked_at: Timestamp,
    pub(crate) current_code_id: u64,
    pub(crate) current_tracked_at: Timestamp,
    pub(crate) family: String,
    pub(crate) sequence: u32,
    pub(crate) migrate_count: u32,
}
