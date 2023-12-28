//! Messages for top-level responses
use cosmwasm_std::Event;

use crate::constants::event_key;

/// Event when an error occurs.
/// Useful in the case of submessages where errors are redacted
/// See: https://github.com/CosmWasm/wasmd/issues/1122
#[derive(Debug)]
pub struct ResponseErrorEvent<T: Into<String>> {
    /// The error message
    pub error: T,
}

impl <T: Into<String>> From<ResponseErrorEvent<T>> for Event {
    fn from(event: ResponseErrorEvent<T>) -> Self {
        Event::new("exec-error")
            .add_attribute(event_key::ERROR, event.error.into())
    }
}