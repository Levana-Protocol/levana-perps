//! Messages and helper data types for the perps protocol.
#![deny(missing_docs)]
#![deny(clippy::as_conversions)]

/// Address Helpers
pub mod cosmwasm;

pub mod error;
pub mod event;

/// Contract result helpers
pub mod result;

pub(crate) mod addr;
pub(crate) mod auth;
pub mod compat;
pub mod direction;
pub mod ibc;
pub mod leverage;
/// Feature-gated logging functionality
pub mod log;
pub mod market_type;
pub mod max_gains;
pub mod namespace;
/// Number type and helpers
pub mod number;
/// Exports very commonly used items into the prelude glob
pub mod prelude;
pub mod price;
pub(crate) mod response;
pub mod storage;
pub mod time;

#[cfg(feature = "bridge")]
pub mod bridge;
pub mod constants;
pub mod contracts;
pub mod shutdown;
pub mod token;

#[test]
fn test_allow_unknown_fields() {
    #[cosmwasm_schema::cw_serde]
    struct Expanded {
        foo: String,
        bar: String,
    }

    impl Default for Expanded {
        fn default() -> Self {
            Expanded {
                foo: "hello".to_string(),
                bar: "world".to_string(),
            }
        }
    }

    #[cosmwasm_schema::cw_serde]
    struct Minimal {
        foo: String,
    }

    let expanded_str = serde_json::to_string(&Expanded::default()).unwrap();
    let expanded: Expanded = serde_json::from_str(&expanded_str).unwrap();
    let minimal: Minimal = serde_json::from_str(&expanded_str).unwrap();

    assert_eq!(expanded.foo, minimal.foo);
}
