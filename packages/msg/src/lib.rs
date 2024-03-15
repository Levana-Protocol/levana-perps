//! Messages and helper data types for the perps protocol.
#![deny(missing_docs)]

#[cfg(feature = "bridge")]
pub mod bridge;
pub mod constants;
pub mod contracts;
pub mod prelude;
pub mod shutdown;
pub mod token;

/// Reexport the shared crate.
///
/// Reexported to simplify the deployment story for external tools. Now they
/// only have to depend on one crate, not two, and avoid accidentally using
/// incompatible versions.
pub use shared;

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
