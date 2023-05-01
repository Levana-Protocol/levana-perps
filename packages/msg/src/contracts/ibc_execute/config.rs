use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, IbcChannel};

#[cw_serde]
pub struct Config {
    /// The IBC channel we're listening to
    /// This is set in the contract handler when the channel is connected.
    pub ibc_channel: Option<IbcChannel>,

    /// The contract we pass messages through to
    pub contract: Addr,

    /// The admin for sending direct execute messages
    pub admin: Addr,
}
