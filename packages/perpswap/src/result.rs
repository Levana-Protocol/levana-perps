use anyhow::Result;
use cosmwasm_std::{to_json_binary, QueryResponse};
use serde::Serialize;
/// Makes it easy to call .query_result() on any Serialize
/// and standardizes so query() entry points also return a ContractResult
pub trait QueryResultExt {
    /// Convert the value to its JSON representation
    fn query_result(&self) -> Result<QueryResponse>;
}
impl<T: Serialize> QueryResultExt for T {
    fn query_result(&self) -> Result<QueryResponse> {
        to_json_binary(self).map_err(|err| err.into())
    }
}
