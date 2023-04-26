use msg::contracts::hatching::{
    config::{Config, ConfigNftBurnContracts},
    entry::InstantiateMsg,
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
    msg: &InstantiateMsg,
) -> Result<()> {
    CONFIG.save(
        store,
        &Config {
            nft_burn_contracts: ConfigNftBurnContracts {
                egg: api.addr_validate(&msg.burn_egg_contract)?,
                dust: api.addr_validate(&msg.burn_dust_contract)?,
            },
            nft_mint_channel: None,
            lvn_grant_channel: None,
        },
    )?;

    Ok(())
}
pub(crate) fn load_config(store: &dyn Storage) -> Result<Config> {
    CONFIG.load(store).map_err(|err| err.into())
}
