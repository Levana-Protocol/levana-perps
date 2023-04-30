use crate::prelude::*;
use cosmwasm_std::{Api, Deps, DepsMut, Env, Storage};

pub(crate) struct State<'a> {
    #[allow(dead_code)] // FIXME remove before production
    pub(crate) api: &'a dyn Api,
    #[allow(dead_code)] // FIXME remove before production
    pub(crate) env: Env,
}

pub(crate) struct StateContext<'a> {
    #[allow(dead_code)] // FIXME remove before production
    pub(crate) storage: &'a mut dyn Storage,
    pub(crate) response: ResponseBuilder,
}

impl<'a> State<'a> {
    pub(crate) fn new(deps: Deps<'a>, env: Env) -> (Self, &dyn Storage) {
        (State { api: deps.api, env }, deps.storage)
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
