//! Messages for the perps factory contract.
//!
//! The factory is responsible for instantiating new markets and providing
//! authentication and protocol-wide information lookup to the other contracts.
// Used by multi_test
pub mod entry;
// Used by contracts/factory/src/contract.rs
pub mod events;
