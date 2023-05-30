use super::{State, StateContext};
use cosmwasm_std::{to_binary, IbcMsg, IbcTimeout};
use msg::contracts::{
    hatching::{
        nft::{Metadata, Trait},
        HatchDetails, NftHatchInfo, dragon_mint::DragonMintExtra,
    },
    ibc_execute_proxy::entry::IbcProxyContractMessages,
};
use serde::{Deserialize, Serialize};
use shared::{ibc::TIMEOUT_SECONDS, prelude::*};

const BABY_DRAGON_EXTRA: Map<&str, DragonMintExtra> = Map::new("baby-dragon-extra");

impl State<'_> {
    pub(crate) fn set_babydragon_extras(
        &self,
        ctx: &mut StateContext,
        extras: Vec<DragonMintExtra>,
    ) -> Result<()> {
        for extra in extras {
            BABY_DRAGON_EXTRA.save(ctx.storage, &extra.id, &extra)?;
        }

        Ok(())
    }

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


    // NFTs are minted via sending an IBC message to a proxy contract on the other chain
    // The proxy contract receives the IbcProxyContractMessages wrapper, unpacks it,
    // and forwards the inner NFT execute messages (encoded as Binary) to the NFT contract
    pub(crate) fn get_nft_mint_proxy_messages(
        &self,
        store: &dyn Storage,
        details: &HatchDetails,
    ) -> Result<IbcProxyContractMessages> {
        let nft_mint_owner = details.nft_mint_owner.to_string();
        Ok(IbcProxyContractMessages(
            get_nft_mint_iter(details)
                .map(|egg| {
                    let extra = BABY_DRAGON_EXTRA
                        .load(store, egg.token_id.as_str())
                        .context("no extra data for egg")?;

                    babydragon_nft_mint_msg(nft_mint_owner.clone(), egg, extra)
                })
                .map(|mint_msg| to_binary(&NftExecuteMsg::Mint(mint_msg?)).map_err(|err| err.into()))
                .collect::<Result<Vec<_>>>()?,
        ))
    }
}


// extracts only those NFTs that are mintable
pub(crate) fn get_nft_mint_iter(details: &HatchDetails) -> impl Iterator<Item = &NftHatchInfo> {
    details.eggs.iter()
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

pub(crate) fn babydragon_nft_mint_msg(owner: String, egg: &NftHatchInfo, extra: DragonMintExtra) -> Result<MintMsg> {
    let mut metadata = Metadata::default();

    let hatching_date = egg.hatch_time.try_into_chrono_datetime()?.format("%Y-%m-%d").to_string();

    let mut attributes:Vec<Trait> = egg
        .burn_metadata
        .attributes
        .as_ref()
        .ok_or_else(|| anyhow!("no attributes"))?
        .iter()
        .filter(|attr| {
            attr.trait_type != "Nesting Date"
        })
        .cloned()
        .collect();

    let dragon_type = &attributes.iter().find(|attr| attr.trait_type == "Dragon Type").context("no dragon type")?.value;
    if *dragon_type != extra.kind {
        bail!("dragon type mismatch for {}: {} != {}", egg.token_id, dragon_type, extra.kind);
    }

    attributes.extend(
        [
            ("Hatching Date", hatching_date),
            ("Eye Color", extra.eye_color.to_string()),
        ]
        .into_iter()
        .map(|(trait_type, value)| Trait {
            display_type: None,
            trait_type: trait_type.to_string(),
            value,
        })
    );

    metadata.image = Some(extra.image_ipfs_url());
    metadata.name = Some(format!("Levana Dragon: #{}", egg.token_id));
    metadata.description = Some("The mighty Levana dragon is a creature of legend, feared and respected by all who know of it. This dragon is a rare and valuable collectible, a symbol of power, strength, and wisdom. It is a reminder that even in the darkest of times, there is always hope.".to_string());
    metadata.attributes = Some(attributes);

    Ok(MintMsg {
        token_id: egg.token_id.to_string(),
        owner,
        extension: metadata,
    })
}
