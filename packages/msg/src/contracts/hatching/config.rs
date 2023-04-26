use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, IbcChannel};

#[cw_serde]
pub struct Config {
    pub nft_burn_contracts: ConfigNftBurnContracts,
    /// The IBC channel for NFT minting.
    /// This is set in the contract handler when the channel is connected.
    pub nft_mint_channel: Option<IbcChannel>,
    /// The IBC channel for LVN granting.
    /// This is set in the contract handler when the channel is connected.
    pub lvn_grant_channel: Option<IbcChannel>,
}

#[cw_serde]
pub struct ConfigNftBurnContracts {
    pub egg: Addr,
    pub dust: Addr,
}
