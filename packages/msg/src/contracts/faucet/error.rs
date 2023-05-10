use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shared::prelude::*;

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct FaucetError {
    pub wait_secs: Decimal256,
}
