use crate::state::*;
use cw_storage_plus::Item;
use perpswap::namespace;

/// The common case of contract owner
const OWNER_ADDR: Item<Addr> = Item::new(namespace::OWNER_ADDR);
/// The admin address for migrations, needed for instantiated vammms
const MIGRATION_ADMIN: Item<Addr> = Item::new(namespace::MIGRATION_ADMIN);
/// DAO address
const DAO_ADDR: Item<Addr> = Item::new(namespace::DAO_ADDR);
/// Kill switch address
const KILL_SWITCH_ADDR: Item<Addr> = Item::new(namespace::KILL_SWITCH_ADDR);
/// Wind down address
const WIND_DOWN_ADDR: Item<Addr> = Item::new(namespace::WIND_DOWN_ADDR);

pub(crate) fn get_owner(store: &dyn Storage) -> Option<Addr> {
    OWNER_ADDR.load(store).ok()
}

pub(crate) fn get_dao(store: &dyn Storage) -> Result<Addr> {
    DAO_ADDR.load(store).map_err(|err| err.into())
}

pub(crate) fn get_admin_migration(store: &dyn Storage) -> Result<Addr> {
    MIGRATION_ADMIN.load(store).map_err(|err| err.into())
}

pub(crate) fn get_kill_switch(store: &dyn Storage) -> Result<Addr> {
    KILL_SWITCH_ADDR.load(store).map_err(|err| err.into())
}

pub(crate) fn get_wind_down(store: &dyn Storage) -> Result<Addr> {
    WIND_DOWN_ADDR.load(store).map_err(|err| err.into())
}

pub(crate) fn set_owner(store: &mut dyn Storage, owner: &Addr) -> Result<()> {
    OWNER_ADDR.save(store, owner).map_err(|err| err.into())
}

pub(crate) fn set_dao(store: &mut dyn Storage, dao: &Addr) -> Result<()> {
    DAO_ADDR.save(store, dao).map_err(|err| err.into())
}

pub(crate) fn set_admin_migration(store: &mut dyn Storage, admin_migration: &Addr) -> Result<()> {
    MIGRATION_ADMIN.save(store, admin_migration)?;

    Ok(())
}

pub(crate) fn set_kill_switch(store: &mut dyn Storage, kill_switch: &Addr) -> Result<()> {
    KILL_SWITCH_ADDR
        .save(store, kill_switch)
        .map_err(|err| err.into())
}

pub(crate) fn set_wind_down(store: &mut dyn Storage, wind_down: &Addr) -> Result<()> {
    WIND_DOWN_ADDR
        .save(store, wind_down)
        .map_err(|err| err.into())
}
