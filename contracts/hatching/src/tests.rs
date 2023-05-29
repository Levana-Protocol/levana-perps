use crate::state::{config::lvn_from_nft_spirit_level, nft_burn::extract_nft_info, nft_mint::babydragon_nft_mint_msg};

use msg::contracts::hatching::{
    nft::{Metadata, Trait},
    NftRarity, NftBurnKind, dragon_mint::DragonMintExtra,
};
use shared::time::Timestamp;

#[test]
fn hatchable_nft() {
    let now = Timestamp::from_seconds(1685362658);

    for kind in &[NftBurnKind::Egg, NftBurnKind::Dust] {
        for rarity in &[
            NftRarity::Common,
            NftRarity::Rare,
            NftRarity::Ancient,
            NftRarity::Legendary,
        ] {
            for spirit_level in &["1.0", "0.1", "1.23", "0.01", "00.02"] {
                let meta = mock_metadata(Some(*kind), Some(*spirit_level), Some(*rarity));
                let info = extract_nft_info("token_id".to_string(), meta, now).unwrap();

                assert_eq!(info.burn_kind, *kind);
                assert_eq!(info.spirit_level, spirit_level.parse().unwrap());
                lvn_from_nft_spirit_level(spirit_level.parse().unwrap(), *kind, *rarity)
                    .unwrap();
            }
        }

        // disallow 0 spirit levels from being hatched
        extract_nft_info(
            "token_id".to_string(),
            mock_metadata(Some(*kind), Some("0.0"), Some(NftRarity::Common)),
            now
        )
        .unwrap_err();
        extract_nft_info(
            "token_id".to_string(),
            mock_metadata(Some(*kind), None, None),
            now
        )
        .unwrap_err();
    }

    // disallow non-egg or dust nfts
    extract_nft_info(
        "token_id".to_string(),
        mock_metadata(None, Some("1.23"), None),
        now
    )
    .unwrap_err();

    // explicit sanity check for LVN calculation
    let meta = mock_metadata(
        Some(NftBurnKind::Egg),
        Some("1.23"),
        Some(NftRarity::Ancient),
    );
    let info = extract_nft_info("token_id".to_string(), meta, now).unwrap();
    // 1.23 * 2.89 = 3.5547
    assert_eq!(info.lvn, "3.5547".parse().unwrap());
    assert_eq!(
        info.lvn,
        lvn_from_nft_spirit_level(
            "1.23".parse().unwrap(),
            NftBurnKind::Egg,
            NftRarity::Ancient
        )
        .unwrap()
    );
}

#[test]
fn egg_to_dragon() {
    let now = Timestamp::from_seconds(1685362658);
    let burn_meta = mock_metadata(Some(NftBurnKind::Egg), Some("1.23"), Some(NftRarity::Common));
    let info = extract_nft_info("42".to_string(), burn_meta, now).unwrap();
    let extra = DragonMintExtra {
        id: "42".to_string(),
        cid: "somehashhere".to_string(),
        eye_color: "Blue".to_string(),
        kind: "Wyvern".to_string(),
    };
    let mint_meta = babydragon_nft_mint_msg("alice".to_string(), &info, extra.clone()).unwrap().extension;

    let expected_mint_meta: Metadata = serde_json::from_str(EXPECTED_DRAGON_META).unwrap();
    assert_eq!(mint_meta.name, expected_mint_meta.name);
    assert_eq!(mint_meta.description, expected_mint_meta.description);
    assert_eq!(mint_meta.image, expected_mint_meta.image);

    let mut attributes = mint_meta.attributes.unwrap();
    let mut expected_attributes = expected_mint_meta.attributes.unwrap();
    attributes.sort_by(|a, b| a.trait_type.cmp(&b.trait_type));
    expected_attributes.sort_by(|a, b| a.trait_type.cmp(&b.trait_type));

    assert_eq!(attributes, expected_attributes);

    assert_eq!(attributes.iter().find_map(|a| {
        if a.trait_type == "Eye Color" {
            Some(&a.value)
        } else {
            None
        }
    }), Some(&extra.eye_color));

    assert_eq!(attributes.iter().find_map(|a| {
        if a.trait_type == "Dragon Type" {
            Some(&a.value)
        } else {
            None
        }
    }), Some(&extra.kind));
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


static EXPECTED_DRAGON_META: &str = r#"{
    "image":"ipfs://somehashhere",
    "image_data":null,
    "external_url":null,
    "description": "The mighty Levana dragon is a creature of legend, feared and respected by all who know of it. This dragon is a rare and valuable collectible, a symbol of power, strength, and wisdom. It is a reminder that even in the darkest of times, there is always hope.",
    "name":"Levana Dragon: #42",
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
            "trait_type":"Hatching Date",
            "value": "2023-05-29"
        },

        {
            "display_type":null,
            "trait_type":"Dragon Type",
            "value":"Wyvern"
        },
        {
            "display_type":null,
            "trait_type":"Rarity",
            "value":"Common"
        },
        {
            "display_type":null,
            "trait_type":"Spirit Level",
            "value":"1.23"
        },
        {
            "display_type":null,
            "trait_type":"Eye Color",
            "value":"Blue"
        }
    ],
    "background_color":null,
    "animation_url":null,
    "youtube_url":null
}"#;