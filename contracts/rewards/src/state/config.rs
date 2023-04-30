use cosmwasm_std::Storage;
use cw_storage_plus::Item;
use msg::contracts::rewards::config::Config;
use shared::prelude::*;

const CONFIG: Item<Config> = Item::new("config");

pub fn update_config(store: &mut dyn Storage, config: Config) -> Result<()> {
    CONFIG.save(store, &config)?;

    Ok(())
}

pub fn load_config(store: &dyn Storage) -> Result<Config> {
    let config = CONFIG.load(store)?;

    Ok(config)
}
