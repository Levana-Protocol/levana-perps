//! Convenience prelude module.
//!
//! This reexports commonly used identifiers for use by contracts and tools.
pub use shared::prelude::*;

pub use crate::contracts::factory::entry::{
    ExecuteMsg as FactoryExecuteMsg, QueryMsg as FactoryQueryMsg,
};
pub use crate::contracts::market::entry::{
    ExecuteMsg as MarketExecuteMsg, QueryMsg as MarketQueryMsg,
};
