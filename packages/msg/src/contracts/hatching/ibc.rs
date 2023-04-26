use std::str::FromStr;

use cosmwasm_schema::cw_serde;
use shared::storage::NumberGtZero;

// position_token builds on the cw721 spec, so we can reuse the Metadata struct
use crate::contracts::position_token::Metadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbcChannelVersion {
    NftMint,
    LvnGrant,
}

impl IbcChannelVersion {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NftMint => "nft-mint-001",
            Self::LvnGrant => "lvn-grant-001",
        }
    }
}
impl FromStr for IbcChannelVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        if s == Self::NftMint.as_str() {
            Ok(Self::NftMint)
        } else if s == Self::LvnGrant.as_str() {
            Ok(Self::LvnGrant)
        } else {
            Err(anyhow::anyhow!("invalid IBC channel version: {}", s))
        }
    }
}

#[cw_serde]
pub enum IbcExecuteMsg {
    MintNfts {
        owner: String,
        nfts: Vec<NftToMint>,
        hatch_id: String,
    },

    GrantLvn {
        address: String,
        amount: NumberGtZero,
        hatch_id: String,
    },
}

#[cw_serde]
pub struct NftToMint {
    pub token_id: String,
    pub metadata: Metadata,
}
