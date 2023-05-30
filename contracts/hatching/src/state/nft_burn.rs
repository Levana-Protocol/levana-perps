use super::{config::lvn_from_nft_spirit_level, State, StateContext};
use msg::contracts::hatching::{
    nft::{AllNftInfoResponse, Metadata},
    NftBurnKind, NftHatchInfo, NftRarity,
};
use serde::{Deserialize, Serialize};
use shared::prelude::*;

impl State<'_> {
    pub(crate) fn burn_nft(
        &self,
        ctx: &mut StateContext,
        owner: &Addr,
        kind: NftBurnKind,
        token_id: String,
    ) -> Result<NftHatchInfo> {
        let nft_info = self.get_nft_info(owner, kind, token_id)?;

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum NftExecuteMsg {
            Burn { token_id: String },
        }

        let contract = match kind {
            NftBurnKind::Egg => &self.config.nft_burn_contracts.egg,
            NftBurnKind::Dust => &self.config.nft_burn_contracts.dust,
        };

        ctx.response_mut().add_execute_submessage_oneshot(
            contract,
            &NftExecuteMsg::Burn {
                token_id: nft_info.token_id.clone(),
            },
        )?;

        Ok(nft_info)
    }

    pub(crate) fn get_nft_info(
        &self,
        owner: &Addr,
        kind: NftBurnKind,
        token_id: String,
    ) -> Result<NftHatchInfo> {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum NftQueryMsg {
            AllNftInfo { token_id: String },
        }

        let contract = match kind {
            NftBurnKind::Egg => &self.config.nft_burn_contracts.egg,
            NftBurnKind::Dust => &self.config.nft_burn_contracts.dust,
        };

        let res: AllNftInfoResponse = self.querier.query_wasm_smart(
            contract.clone(),
            &NftQueryMsg::AllNftInfo {
                token_id: token_id.clone(),
            },
        )?;

        if res.access.owner != owner {
            bail!(
                "Not the token owner (owner is {}, attempted hatcher is {})",
                res.access.owner,
                owner
            );
        }

        let info = extract_nft_info(token_id, res.info.extension, self.now())?;

        Ok(info)
    }
}

pub(crate) fn extract_nft_info(
    token_id: String,
    meta: Metadata,
    now: Timestamp,
) -> Result<NftHatchInfo> {
    let kind = {
        let name = match &meta.name {
            Some(name) => name.to_lowercase(),
            None => {
                bail!("no name in metadata");
            }
        };
        if name.contains("egg") {
            NftBurnKind::Egg
        } else if name.contains("dust") {
            NftBurnKind::Dust
        } else {
            bail!("NFT is not an egg or dust (name: {}", name);
        }
    };

    match &meta.attributes {
        None => {
            bail!("no attributes in metadata");
        }
        Some(attributes) => {
            let spirit_level: NumberGtZero = attributes
                .iter()
                .find(|a| a.trait_type == "Spirit Level")
                .ok_or_else(|| anyhow!("NFT is not hatchable (no spirit level)"))?
                .value
                .parse()?;

            let rarity: NftRarity = attributes
                .iter()
                .find_map(|a| {
                    if a.trait_type == "Rarity" {
                        match a.value.as_str() {
                            "Legendary" => Some(NftRarity::Legendary),
                            "Ancient" => Some(NftRarity::Ancient),
                            "Rare" => Some(NftRarity::Rare),
                            "Common" => Some(NftRarity::Common),
                            _ => None,
                        }
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow!("NFT is not hatchable (no rarity)"))?;

            let lvn = lvn_from_nft_spirit_level(spirit_level, kind, rarity)?;

            Ok(NftHatchInfo {
                spirit_level,
                rarity,
                lvn,
                token_id,
                burn_kind: kind,
                burn_metadata: meta,
                hatch_time: now,
            })
        }
    }
}
