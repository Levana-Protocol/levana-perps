use super::{State, StateContext};
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
        owner: Addr,
        kind: NftBurnKind,
        token_id: String,
    ) -> Result<NftHatchInfo> {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum NftQueryMsg {
            AllNftInfo { token_id: String },
            NftInfo { token_id: String },
        }
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum NftExecuteMsg {
            Burn { token_id: String },
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

        ctx.response_mut().add_execute_submessage_oneshot(
            contract,
            &NftExecuteMsg::Burn {
                token_id: token_id.clone(),
            },
        )?;

        let info = extract_nft_info(token_id, res.info.extension)?;

        Ok(info)
    }
}

fn extract_nft_info(token_id: String, meta: Metadata) -> Result<NftHatchInfo> {
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

            let lvn_multiplier = match (kind, rarity) {
                (NftBurnKind::Egg, NftRarity::Legendary) => "3.13",
                (NftBurnKind::Egg, NftRarity::Ancient) => "2.89",
                (NftBurnKind::Egg, NftRarity::Rare) => "2.65",
                (NftBurnKind::Egg, NftRarity::Common) => "2.41",
                (NftBurnKind::Dust, NftRarity::Legendary) => "2.77",
                (NftBurnKind::Dust, NftRarity::Ancient) => "2.65",
                (NftBurnKind::Dust, NftRarity::Rare) => "2.53",
                (NftBurnKind::Dust, NftRarity::Common) => "2.17",
            };
            let lvn_multiplier: NumberGtZero = lvn_multiplier.parse()?;

            let lvn = NumberGtZero::try_from_number(
                spirit_level.into_number() * lvn_multiplier.into_number(),
            )
            .context("cannot have non-zero lvn")?;

            Ok(NftHatchInfo {
                spirit_level,
                rarity,
                lvn,
                token_id,
                burn_kind: kind,
                burn_metadata: meta,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::nft_burn::extract_nft_info;

    use super::NftBurnKind;
    use msg::contracts::hatching::{
        nft::{Metadata, Trait},
        NftRarity,
    };

    #[test]
    fn hatchable_nft() {
        for kind in &[NftBurnKind::Egg, NftBurnKind::Dust] {
            for rarity in &[
                NftRarity::Common,
                NftRarity::Rare,
                NftRarity::Ancient,
                NftRarity::Legendary,
            ] {
                for spirit_level in &["1.0", "0.1", "1.23", "0.01", "00.02"] {
                    let meta = mock_metadata(Some(*kind), Some(*spirit_level), Some(*rarity));
                    let info = extract_nft_info("token_id".to_string(), meta).unwrap();
                    assert_eq!(info.burn_kind, *kind);
                    assert_eq!(info.spirit_level, spirit_level.parse().unwrap());
                }
            }

            // disallow 0 spirit levels from being hatched
            extract_nft_info(
                "token_id".to_string(),
                mock_metadata(Some(*kind), Some("0.0"), Some(NftRarity::Common)),
            )
            .unwrap_err();
            extract_nft_info(
                "token_id".to_string(),
                mock_metadata(Some(*kind), None, None),
            )
            .unwrap_err();
        }

        // disallow non-egg or dust nfts
        extract_nft_info(
            "token_id".to_string(),
            mock_metadata(None, Some("1.23"), None),
        )
        .unwrap_err();

        // explicit sanity check for LVN calculation
        let meta = mock_metadata(
            Some(NftBurnKind::Egg),
            Some("1.23"),
            Some(NftRarity::Ancient),
        );
        let info = extract_nft_info("token_id".to_string(), meta).unwrap();
        // 1.23 * 2.89 = 3.5547
        assert_eq!(info.lvn, "3.5547".parse().unwrap());
    }

    fn mock_metadata(
        kind: Option<NftBurnKind>,
        spirit_level: Option<&str>,
        rarity: Option<NftRarity>,
    ) -> Metadata {
        let mut meta: Metadata = serde_json::from_str(match kind {
            Some(NftBurnKind::Egg) => EGG_META,
            Some(NftBurnKind::Dust) => DUST_META,
            None => OTHER_META,
        })
        .unwrap();

        if let Some(value) = spirit_level {
            meta.attributes.as_mut().unwrap().push(Trait {
                display_type: None,
                trait_type: "Spirit Level".to_string(),
                value: value.to_string(),
            });
        }

        if let Some(value) = rarity {
            meta.attributes.as_mut().unwrap().push(Trait {
                display_type: None,
                trait_type: "Rarity".to_string(),
                value: format!("{:?}", value),
            });
        }

        meta
    }

    static DUST_META: &str = r#"{
        "image": null,
        "image_data":null,
        "external_url":null,
        "description":"Evolutionary Rare Meteor Dust NFT, stage 2 of the Levana Dragons adventure.",
        "name":"Levana Dragons: Rare Meteor Dust",
        "attributes":[
            {
                "display_type":null,
                "trait_type":"Type",
                "value":"Meteor Dust"
            },
            {
                "display_type":null,
                "trait_type":"Dust Volume",
                "value":"Quarter"
            },
            {
                "display_type":null,
                "trait_type":"Essence",
                "value":"Psychic"
            },
            {
                "display_type":null,
                "trait_type":"Rare Gem",
                "value":"Cinnabar"
            },
            {
                "display_type":null,
                "trait_type":"Common Gem",
                "value":"Azurite"
            },
            {
                "display_type":null,
                "trait_type":"Rare Composition",
                "value":"Sulfur"
            },
            {
                "display_type":null,
                "trait_type":"Common Composition",
                "value":"Silicon"
            }
        ],
        "background_color":null,
        "animation_url":null,
        "youtube_url":null
    }"#;

    static EGG_META: &str = r#"{
        "image":"ipfs://blah",
        "image_data":null,
        "external_url":null,
        "description":"Evolutionary Rare Nested Egg NFT, stage 3 of the Levana Dragons adventure.",
        "name":"Levana Dragons: Rare Nested Egg #0",
        "attributes":[
            {
                "display_type":null,
                "trait_type":"Stage",
                "value":"Nested Egg"
            },
            {
                "display_type":null,
                "trait_type":"Origin",
                "value":"Southern hemisphere subterranean caves"
            },
            {
                "display_type":null,
                "trait_type":"Essence",
                "value":"Electric"
            },
            {
                "display_type":null,
                "trait_type":"Rare Composition",
                "value":"Nitrogen"
            },
            {
                "display_type":null,
                "trait_type":"Common Composition",
                "value":"Sodium"
            },
            {
                "display_type":null,
                "trait_type":"Family",
                "value":"Oquania"
            },
            {
                "display_type":null,
                "trait_type":"Genus",
                "value":"Chaos"
            },
            {
                "display_type":null,
                "trait_type":"Affecting Moon",
                "value":"Sao"
            },
            {
                "display_type":null,
                "trait_type":"Lucky Number",
                "value":"1"
            },
            {
                "display_type":null,
                "trait_type":"Constellation",
                "value":"Cerberus"
            },
            {
                "display_type":null,
                "trait_type":"Nesting Date",
                "value":"2472-02-01"
            },
            {
                "display_type":null,
                "trait_type":"Dragon Type",
                "value":"Wyvern"
            }
        ],
        "background_color":null,
        "animation_url":null,
        "youtube_url":null
    }"#;

    static OTHER_META: &str = r#"{
        "image":"ipfs://blah",
        "image_data":null,
        "external_url":null,
        "description":"Nothing interesting here",
        "name":"Other",
        "attributes":[ ],
        "background_color":null,
        "animation_url":null,
        "youtube_url":null
    }"#;
}
