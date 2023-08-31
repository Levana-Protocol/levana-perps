use crate::prelude::*;
use cosmwasm_std::{
    Api, Binary, ContractResult, Empty, Event, QuerierWrapper, QueryRequest, StdError,
    SystemResult, WasmQuery,
};

/// Like [cosmwasm_std::Order] but serialized as a string
/// and with a schema export
#[cw_serde]
#[derive(Eq, Copy)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum OrderInMessage {
    /// Ascending order
    Ascending,
    /// Descending order
    Descending,
}

impl From<OrderInMessage> for cosmwasm_std::Order {
    fn from(order: OrderInMessage) -> Self {
        match order {
            OrderInMessage::Ascending => Self::Ascending,
            OrderInMessage::Descending => Self::Descending,
        }
    }
}

/// Extract an attribute for the given parameters from an Event
fn extract_attribute<'a>(ty: &str, key: &str, events: &'a [Event]) -> Option<&'a str> {
    events
        .iter()
        .find(|e| e.ty == ty)
        .and_then(|ev| ev.attributes.iter().find(|a| a.key == key))
        .map(|attr| attr.value.as_str())
}

// https://github.com/CosmWasm/wasmd/blob/main/EVENTS.md#standard-events-in-xwasm
// but in practice it seems it's sometimes different
/// Extract contract address from an instantiation event
pub fn extract_instantiated_addr(api: &dyn Api, events: &[Event]) -> Result<Addr> {
    for (ty, key) in [
        (
            "cosmwasm.wasm.v1.EventContractInstantiated",
            "contract_address",
        ),
        ("instantiate", "_contract_address"),
        ("instantiate", "_contract_addr"),
        ("wasm", "contract_address"),
        ("instantiate_contract", "contract_address"),
    ] {
        if let Some(addr) = extract_attribute(ty, key, events) {
            let addr = addr
                .strip_prefix('\"')
                .and_then(|s| s.strip_suffix('\"'))
                .unwrap_or(addr);
            let addr = api.addr_validate(addr)?;
            return Ok(addr);
        }
    }
    Err(anyhow!("Couldn't find instantiated address"))
}

/// Make a smart query, but do not parse the binary results as JSON.
///
/// Useful for proxies where we need to pass along the binary results directly.
pub fn smart_query_no_parse(
    querier: &QuerierWrapper<Empty>,
    contract_addr: impl Into<String>,
    msg: &impl serde::Serialize,
) -> anyhow::Result<Binary> {
    let request: QueryRequest<Empty> = WasmQuery::Smart {
        contract_addr: contract_addr.into(),
        msg: cosmwasm_std::to_binary(msg)?,
    }
    .into();
    let raw = cosmwasm_std::to_vec(&request).map_err(|serialize_err| {
        StdError::generic_err(format!("Serializing QueryRequest: {}", serialize_err))
    })?;
    match querier.raw_query(&raw) {
        SystemResult::Err(system_err) => Err(anyhow!("Querier system error: {}", system_err)),
        SystemResult::Ok(ContractResult::Err(contract_err)) => {
            Err(anyhow!("Querier contract error: {}", contract_err))
        }
        SystemResult::Ok(ContractResult::Ok(value)) => Ok(value),
    }
}
