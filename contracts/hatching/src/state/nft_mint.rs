use super::{State, StateContext};
use cosmwasm_std::{to_binary, IbcMsg, IbcTimeout};
use msg::contracts::{
    hatching::{
        ibc::{IbcExecuteMsg, NftToMint},
        HatchDetails, NftHatchInfo,
    },
    position_token::{Metadata, Trait},
};
use shared::{ibc::TIMEOUT_SECONDS, prelude::*};

impl State<'_> {
    pub(crate) fn send_mint_nfts_ibc_message(
        &self,
        ctx: &mut StateContext,
        hatch_id: u64,
        owner: &Addr,
        nfts: Vec<NftToMint>,
    ) -> Result<()> {
        // outbound IBC message, where packet is then received on other chain
        let channel_id = self
            .config
            .nft_mint_channel
            .as_ref()
            .context("no nft mint channel")?
            .endpoint
            .channel_id
            .clone();

        let msg = IbcExecuteMsg::MintNfts {
            owner: owner.to_string(),
            nfts,
            hatch_id: hatch_id.to_string(),
        };

        ctx.response_mut().add_message(IbcMsg::SendPacket {
            channel_id,
            data: to_binary(&msg)?,
            timeout: IbcTimeout::with_timestamp(self.env.block.time.plus_seconds(TIMEOUT_SECONDS)),
        });

        Ok(())
    }
}

pub fn get_nfts_to_mint(details: &HatchDetails) -> Vec<NftToMint> {
    details.eggs.iter().map(babydragon_nft).collect()
}

fn babydragon_nft(egg: &NftHatchInfo) -> NftToMint {
    let mut metadata = Metadata::default();

    // TODO - finalize the real NFT metadata
    let attributes = [
        ("Spirit Level", egg.spirit_level.to_string()),
        ("Egg Id", egg.token_id.to_string()),
    ]
    .map(|(trait_type, value)| Trait {
        display_type: None,
        trait_type: trait_type.to_string(),
        value,
    });

    metadata.name = Some(format!("Baby Dragon {}", egg.token_id));
    metadata.attributes = Some(attributes.into_iter().collect());

    NftToMint {
        token_id: egg.token_id.to_string(),
        metadata,
    }
}
