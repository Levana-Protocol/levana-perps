pub mod config;
pub mod ibc;
pub mod send;

use msg::contracts::ibc_execute::config::Config;

use cw2::get_contract_version;
use shared::prelude::*;

use cosmwasm_std::{Api, Deps, DepsMut, Empty, Env, QuerierWrapper, Storage};

use self::config::load_config;

pub(crate) struct State<'a> {
    pub(crate) _api: &'a dyn Api,
    pub(crate) _env: Env,
    pub(crate) _querier: QuerierWrapper<'a, Empty>,
    pub(crate) config: Config,
}

pub(crate) struct StateContext<'a> {
    pub(crate) storage: &'a mut dyn Storage,
    pub(crate) response: ResponseBuilder,
}

impl<'a> State<'a> {
    pub(crate) fn new(deps: Deps<'a>, env: Env) -> Result<(Self, &dyn Storage)> {
        Ok((
            State {
                config: load_config(deps.storage)?,
                _api: deps.api,
                _env: env,
                _querier: deps.querier,
            },
            deps.storage,
        ))
    }
}

impl<'a> StateContext<'a> {
    pub(crate) fn new(deps: DepsMut<'a>, env: Env) -> Result<(State<'a>, Self)> {
        let contract_version = get_contract_version(deps.storage)?;
        Ok((
            State {
                config: load_config(deps.storage)?,
                _api: deps.api,
                _env: env,
                _querier: deps.querier,
            },
            StateContext {
                storage: deps.storage,
                response: ResponseBuilder::new(contract_version),
            },
        ))
    }
    pub(crate) fn response_mut(&mut self) -> &mut ResponseBuilder {
        &mut self.response
    }
}
