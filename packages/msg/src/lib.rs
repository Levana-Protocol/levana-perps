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
