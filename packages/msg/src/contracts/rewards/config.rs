use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal256};

#[cw_serde]
pub struct Config {
    /// The portion of rewards that are sent to the user immediately after receiving LVN tokens.
    /// Defined as a ratio between 0 and 1.
    pub immediately_transferable: Decimal256,
    /// The denom for the LVN token which will be used for rewards
    pub token_denom: String,
    /// The amount of time it takes rewards to unlock linearly, defined in seconds
    pub unlock_duration_seconds: u32,
    /// The factory contract addr, used for auth
    pub factory_addr: Addr,
}
