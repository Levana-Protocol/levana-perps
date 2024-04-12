use msg::contracts::hatching::{
    config::{Config, ConfigNftBurnContracts},
    entry::InstantiateMsg,
    NftBurnKind, NftRarity,
};
use shared::prelude::*;

use super::{State, StateContext};

const CONFIG: Item<Config> = Item::new("config");

impl State<'_> {
    pub(crate) fn save_config(&self, ctx: &mut StateContext) -> Result<()> {
        CONFIG
            .save(ctx.storage, &self.config)
            .map_err(|err| err.into())
    }
}

pub(crate) fn init_config(
    store: &mut dyn Storage,
    api: &dyn Api,
    admin: Addr,
    msg: &InstantiateMsg,
) -> Result<()> {
    CONFIG.save(
        store,
        &Config {
            admin,
            nft_burn_contracts: ConfigNftBurnContracts {
                egg: msg.burn_egg_contract.validate(api)?,
                dust: msg.burn_dust_contract.validate(api)?,
            },
            profile_contract: msg.profile_contract.validate(api)?,
            nft_mint_channel: None,
            lvn_grant_channel: None,
        },
    )?;

    Ok(())
}
pub(crate) fn load_config(store: &dyn Storage) -> Result<Config> {
    CONFIG.load(store).map_err(|err| err.into())
}

pub(crate) fn lvn_from_profile_spirit_level(spirit_level: NumberGtZero) -> Result<NumberGtZero> {
    Decimal256::from_str("2.41")
        .map_err(|err| err.into())
        .map(|multiplier| spirit_level.into_decimal256() * multiplier)
        .and_then(|lvn| NumberGtZero::try_from_decimal(lvn).context("lvn cannot be zero"))
}

pub(crate) fn lvn_from_nft_spirit_level(
    spirit_level: NumberGtZero,
    kind: NftBurnKind,
    rarity: NftRarity,
) -> Result<NumberGtZero> {
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

    NumberGtZero::try_from_number((spirit_level.into_number() * lvn_multiplier.into_number())?)
        .context("cannot have non-zero lvn")
}
