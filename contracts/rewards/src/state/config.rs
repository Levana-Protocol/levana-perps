use super::State;
use cosmwasm_std::Storage;
use cw_storage_plus::Item;
use msg::contracts::rewards::config::Config;
use msg::contracts::rewards::entry::ConfigUpdate;
use shared::prelude::*;

const CONFIG: Item<Config> = Item::new("config");

impl State<'_> {
    pub(crate) fn save_config(&self, store: &mut dyn Storage) -> Result<()> {
        CONFIG.save(store, &self.config)?;
        Ok(())
    }
}

pub(crate) fn load_config(store: &dyn Storage) -> Result<Config> {
    let config = CONFIG.load(store)?;

    Ok(config)
}

pub(crate) fn config_init(
    api: &dyn Api,
    storage: &mut dyn Storage,
    config: ConfigUpdate,
) -> Result<()> {
    let factory_addr = api.addr_validate(&config.factory_addr)?;

    CONFIG.save(
        storage,
        &Config {
            immediately_transferable: config.immediately_transferable,
            token_denom: config.token_denom,
            unlock_duration_seconds: config.unlock_duration_seconds,
            factory_addr,
            lvn_grant_channel: None,
        },
    )?;

    Ok(())
}

pub(crate) fn update_config(
    mut state: State,
    storage: &mut dyn Storage,
    config: ConfigUpdate,
) -> Result<()> {
    let factory_addr = state.api.addr_validate(&config.factory_addr)?;

    state.config.immediately_transferable = config.immediately_transferable;
    state.config.token_denom = config.token_denom;
    state.config.unlock_duration_seconds = config.unlock_duration_seconds;
    state.config.factory_addr = factory_addr;

    state.save_config(storage)?;

    Ok(())
}
