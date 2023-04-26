//! Messages for the perps market contract.
//!
//! The market contract contains the vast majority of functionality within
//! perps, and handles trading, liquidity providing, history, and more.
pub mod config;
pub mod crank;
pub mod delta_neutrality_fee;
pub mod entry;
pub mod fees;
pub mod history;
pub mod liquidity;
pub mod order;
pub mod position;
pub mod spot_price;
