use crate::state::{config::lvn_from_nft_spirit_level, nft_burn::extract_nft_info};

use msg::contracts::hatching::{
    nft::{Metadata, Trait},
    NftRarity, NftBurnKind,
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
                lvn_from_nft_spirit_level(spirit_level.parse().unwrap(), *kind, *rarity)
                    .unwrap();
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