use cosmwasm_schema::QueryResponses;
use perpswap::prelude::*;

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    /// Track the upload of a WASM file
    CodeId {
        /// User friendly string describing the contract
        contract_type: String,
        /// Code ID of the uploaded file
        code_id: u64,
        /// SHA256 hash to uniquely identify a file
        hash: String,
        /// Git commit that generated this code, if known
        gitrev: Option<String>,
    },
    /// Track the instantiation of a new contract
    ///
    /// Will automatically assign a unique identifier from the deployment family
    Instantiate {
        code_id: u64,
        address: String,
        /// Family of the deployment.
        ///
        /// This can be things like osmodev or dragonci. The idea would be that there
        /// can be a series of contracts in this family, and the latest one is the
        /// current true deployment.
        ///
        /// Each individual instantiation will get a unique identifier based on this name.
        family: String,
    },
    /// Track the migration of an existing contract to a new code ID.
    ///
    /// This information is already tracked on the blockchain, it's just
    /// convenient to have it here too.
    Migrate { new_code_id: u64, address: String },
    /// Add an administrator address
    AddAdmin { address: String },
    /// Remove an administrator address
    RemoveAdmin { address: String },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Lookup the code by ID
    #[returns(CodeIdResp)]
    CodeById { code_id: u64 },
    /// Lookup the code by hash
    #[returns(CodeIdResp)]
    CodeByHash { hash: String },
    /// Get the contract information for the given contract address
    #[returns(ContractResp)]
    ContractByAddress { address: String },
    /// Get the contract information for the latest contract in a family
    #[returns(ContractResp)]
    ContractByFamily {
        /// This is derived from the Code ID during the Instantiate call
        contract_type: String,
        family: String,
        /// Unique identifier within the series to look up.
        ///
        /// If omitted, gets the most recent
        sequence: Option<u32>,
    },
}

#[cw_serde]
pub enum CodeIdResp {
    NotFound {},
    Found {
        contract_type: String,
        code_id: u64,
        hash: String,
        tracked_at: Timestamp,
        gitrev: Option<String>,
    },
}

#[cw_serde]
pub enum ContractResp {
    NotFound {},
    Found {
        address: String,
        contract_type: String,
        original_code_id: u64,
        /// When we received the Instantiate call
        original_tracked_at: Timestamp,
        current_code_id: u64,
        /// When we received the most recent instantiate or migrate
        current_tracked_at: Timestamp,
        family: String,
        sequence: u32,
        /// How many times have we been migrated?
        migrate_count: u32,
    },
}
