pub mod config;
mod ibc;
pub mod rewards;

use self::config::load_config;
use cosmwasm_std::{Api, Deps, DepsMut, Empty, Env, QuerierWrapper, Storage};
use cw2::get_contract_version;
use msg::contracts::rewards::config::Config;
use shared::prelude::*;

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) env: Env,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
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
                api: deps.api,
                env,
                querier: deps.querier,
                config: load_config(deps.storage)?,
            },
            deps.storage,
        ))
    }

    pub(crate) fn now(&self) -> Timestamp {
        self.env.block.time.into()
    }
}

impl<'a> StateContext<'a> {
    pub(crate) fn new(deps: DepsMut<'a>, env: Env) -> Result<(State<'a>, Self)> {
        let contract_version = get_contract_version(deps.storage)?;
        Ok((
            State {
                api: deps.api,
                env,
                querier: deps.querier,
                config: load_config(deps.storage)?,
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
