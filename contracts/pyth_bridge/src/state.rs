// re-exporting
pub(crate) mod market;
pub(crate) mod pyth;

use cosmwasm_std::{Deps, DepsMut, Empty, Env, QuerierWrapper, Storage};
use msg::contracts::pyth_bridge::entry::Config;
use shared::prelude::*;

use self::pyth::get_pyth_config;

pub(crate) struct State<'a> {
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) config: Config,
    pub(crate) env: Env,
}

pub(crate) struct StateContext<'a> {
    pub(crate) storage: &'a mut dyn Storage,
    pub(crate) response: ResponseBuilder,
}

impl<'a> State<'a> {
    pub(crate) fn new(deps: Deps<'a>, env: Env) -> Result<(Self, &dyn Storage)> {
        let config = get_pyth_config(deps.storage)?;
        Ok((
            State {
                querier: deps.querier,
                config,
                env,
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
        let config = get_pyth_config(deps.storage)?;
        Ok((
            State {
                querier: deps.querier,
                config,
                env,
            },
            StateContext {
                storage: deps.storage,
                response: ResponseBuilder::new(contract_version),
            },
        ))
    }
}
