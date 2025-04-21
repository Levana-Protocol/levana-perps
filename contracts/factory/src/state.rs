// re-exporting
pub(crate) mod all_contracts;
pub(crate) mod auth;
pub(crate) mod code_ids;
pub(crate) mod copy_trading;
pub(crate) mod countertrade;
pub(crate) mod label;
pub(crate) mod liquidity_token;
pub(crate) mod market;
pub(crate) mod position_token;
pub(crate) mod referrer;
pub(crate) mod reply;
pub(crate) mod shutdown;

use cosmwasm_std::{Addr, Api, Deps, DepsMut, Env, Storage};
use perpswap::prelude::*;

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) env: Env,
}

pub(crate) struct StateContext<'a> {
    pub(crate) storage: &'a mut dyn Storage,
    pub(crate) response: ResponseBuilder,
}

impl<'a> State<'a> {
    pub(crate) fn new(deps: Deps<'a>, env: Env) -> (Self, &'a dyn Storage) {
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
