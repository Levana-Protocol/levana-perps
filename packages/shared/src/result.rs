use anyhow::Result;
use cosmwasm_std::{to_binary, QueryResponse};
use serde::Serialize;
/// Makes it easy to call .query_result() on any Serialize
/// and standardizes so query() entry points also return a ContractResult
pub trait QueryResultExt {
    /// Convert the value to its JSON representation
    fn query_result(&self) -> Result<QueryResponse>;
}
impl<T: Serialize> QueryResultExt for T {
    fn query_result(&self) -> Result<QueryResponse> {
        to_binary(self).map_err(|err| err.into())
    }
}

/// Makes it easy to create a Response where all the values
/// are attributes, like a HashMap.
///
/// example:
///
/// attr_map!{
///     "color" => "orange",
///     "amount" => 2
/// }
///
/// is equivilent to
/// Response::new()
///     .add_attribute("color", "orange")
///     .add_attribute("amount", 2)
#[macro_export]
macro_rules! attr_map {
            ($($key:expr => $val:expr),* ,) => (
                $crate::attr_map!($($key => $val),*)
            );
            ($($key:expr => $val:expr),*) => ({
                ::cosmwasm_std::Response::new()
                $( .add_attribute($key, $val) )*
            });
}
