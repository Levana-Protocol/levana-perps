use msg::contracts::ibc_execute::{config::Config, entry::InstantiateMsg};
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
            ibc_channel: None,
            contract: msg.contract.validate(api)?,
        },
    )?;

    Ok(())
}
pub(crate) fn load_config(store: &dyn Storage) -> Result<Config> {
    CONFIG.load(store).map_err(|err| err.into())
}
