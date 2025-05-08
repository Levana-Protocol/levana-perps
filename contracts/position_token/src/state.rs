// re-exporting
mod market;

use cosmwasm_std::{Addr, Deps, DepsMut, Empty, Env, QuerierWrapper, Storage};
use cw_storage_plus::Item;
use perpswap::namespace;
use perpswap::prelude::*;

/// The factory address
const FACTORY_ADDR: Item<Addr> = Item::new(namespace::FACTORY_ADDR);

pub(crate) struct State<'a> {
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) factory_address: Addr,
}

pub(crate) struct StateContext<'a> {
    pub(crate) storage: &'a mut dyn Storage,
    pub(crate) response: ResponseBuilder,
}

impl<'a> State<'a> {
    pub(crate) fn new(deps: Deps<'a>, _env: Env) -> Result<(Self, &dyn Storage)> {
        let factory_address = FACTORY_ADDR.load(deps.storage)?;
        Ok((
            State {
                querier: deps.querier,
                factory_address,
            },
            deps.storage,
        ))
    }
}

impl<'a> StateContext<'a> {
    pub(crate) fn new(deps: DepsMut<'a>, _env: Env) -> Result<(State<'a>, Self)> {
        let contract_version = get_contract_version(deps.storage)?;
        let factory_address = FACTORY_ADDR.load(deps.storage)?;
        Ok((
            State {
                querier: deps.querier,
                factory_address,
            },
            StateContext {
                storage: deps.storage,
                response: ResponseBuilder::new(contract_version),
            },
        ))
    }
}

pub(crate) fn set_factory_addr(store: &mut dyn Storage, factory_addr: &Addr) -> Result<()> {
    FACTORY_ADDR.save(store, factory_addr)?;

    Ok(())
}
