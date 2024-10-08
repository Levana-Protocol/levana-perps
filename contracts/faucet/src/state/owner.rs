use crate::state::*;
use anyhow::Result;
use cosmwasm_std::Addr;
use cw_storage_plus::Map;
use perpswap::namespace;

const ADMINS: Map<&Addr, ()> = Map::new(namespace::OWNER_ADDR);

pub(crate) fn is_admin(store: &dyn Storage, addr: &Addr) -> bool {
    ADMINS.has(store, addr)
}

pub(crate) fn add_admin(store: &mut dyn Storage, admin: &Addr) -> Result<()> {
    anyhow::ensure!(!ADMINS.has(store, admin));
    ADMINS.save(store, admin, &())?;
    Ok(())
}

pub(crate) fn remove_admin(store: &mut dyn Storage, admin: &Addr) -> Result<()> {
    anyhow::ensure!(ADMINS.has(store, admin));
    ADMINS.remove(store, admin);
    Ok(())
}

pub(crate) fn get_all_admins(store: &dyn Storage) -> Result<Vec<Addr>> {
    ADMINS
        .keys(store, None, None, cosmwasm_std::Order::Ascending)
        .collect::<Result<_, _>>()
        .map_err(|e| e.into())
}
