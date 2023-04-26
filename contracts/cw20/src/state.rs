// re-exporting
mod cw20;
mod trading_competition;
pub use trading_competition::*;

use cosmwasm_std::{Api, Deps, DepsMut, Env, Storage};
use msg::prelude::*;

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) env: Env,
}

pub(crate) struct StateContext<'a> {
    pub(crate) storage: &'a mut dyn Storage,
    pub(crate) response: ResponseBuilder,
}

impl<'a> State<'a> {
    pub(crate) fn new(deps: Deps<'a>, env: Env) -> Result<(Self, &dyn Storage)> {
        Ok((State { api: deps.api, env }, deps.storage))
    }
}

impl<'a> StateContext<'a> {
    pub(crate) fn new(deps: DepsMut<'a>, env: Env) -> Result<(State<'a>, Self)> {
        let contract_version = get_contract_version(deps.storage)?;
        Ok((
            State { api: deps.api, env },
            StateContext {
                storage: deps.storage,
                response: ResponseBuilder::new(contract_version),
            },
        ))
    }
}
