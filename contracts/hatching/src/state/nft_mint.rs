use super::{State, StateContext};
use cosmwasm_std::{to_binary, IbcMsg, IbcTimeout};
use msg::contracts::{
    hatching::{
        nft::{Metadata, Trait},
        HatchDetails, NftHatchInfo,
    },
    ibc_execute::entry::IbcProxyContractMessages,
};
use serde::{Deserialize, Serialize};
use shared::{ibc::TIMEOUT_SECONDS, prelude::*};

impl State<'_> {
    pub(crate) fn send_mint_nfts_ibc_message(
        &self,
        ctx: &mut StateContext,
        nfts_to_mint: IbcProxyContractMessages,
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

        ctx.response_mut().add_message(IbcMsg::SendPacket {
            channel_id,
            data: to_binary(&nfts_to_mint)?,
            timeout: IbcTimeout::with_timestamp(self.env.block.time.plus_seconds(TIMEOUT_SECONDS)),
        });

        Ok(())
    }
}

// NFTs are minted via sending an IBC message to a proxy contract on the other chain
// The proxy contract receives the IbcProxyContractMessages wrapper, unpacks it,
// and forwards the inner NFT execute messages (encoded as Binary) to the NFT contract
pub(crate) fn get_nfts_to_mint(
    details: &HatchDetails,
    hatch_id: u64,
) -> Result<IbcProxyContractMessages> {
    let nft_mint_owner = details.nft_mint_owner.to_string();
    Ok(IbcProxyContractMessages(
        details
            .eggs
            .iter()
            .map(|egg| babydragon_nft_mint_msg(nft_mint_owner.clone(), hatch_id, egg))
            .map(|mint_msg| to_binary(&NftExecuteMsg::Mint(mint_msg)).map_err(|err| err.into()))
            .collect::<Result<Vec<_>>>()?,
    ))
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NftExecuteMsg {
    /// Mint a new NFT, can only be called by the contract minter
    Mint(MintMsg),
}

// NOTE - this is currently
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct MintMsg {
    /// Unique ID of the NFT
    pub token_id: String,
    /// The owner of the newly minted NFT
    pub owner: String,
    /// Any custom extension used by this contract
    pub extension: Metadata,
}

impl MintMsg {
    pub fn extract_hatch_id(&self) -> Result<u64> {
        self.extension
            .attributes
            .as_ref()
            .context("no attributes")?
            .iter()
            .find_map(|trait_| {
                if trait_.trait_type == "Hatch Id" {
                    trait_.value.parse::<u64>().ok()
                } else {
                    None
                }
            })
            .context("no hatch id")
    }
}

fn babydragon_nft_mint_msg(owner: String, hatch_id: u64, egg: &NftHatchInfo) -> MintMsg {
    let mut metadata = Metadata::default();

    // TODO - finalize the real NFT metadata
    let attributes = [
        ("Spirit Level", egg.spirit_level.to_string()),
        ("Egg Id", egg.token_id.to_string()),
        ("Hatch Id", hatch_id.to_string()),
    ]
    .map(|(trait_type, value)| Trait {
        display_type: None,
        trait_type: trait_type.to_string(),
        value,
    });

    metadata.image = Some(format!("ipfs://example/{}.png", egg.token_id));
    metadata.name = Some(format!("Baby Dragon {}", egg.token_id));
    metadata.description = Some("A cute little baby dragon fresh out of the cave".to_string());
    metadata.attributes = Some(attributes.into_iter().collect());

    MintMsg {
        token_id: egg.token_id.to_string(),
        owner,
        extension: metadata,
    }
}
