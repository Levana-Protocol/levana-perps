// re-exporting
mod faucet;
pub(crate) use faucet::*;
pub(crate) mod owner;
use perpswap::prelude::*;
mod trading_competition;
pub(crate) use trading_competition::*;
pub(crate) mod tokens;
use cosmwasm_std::{Api, Deps, DepsMut, Empty, Env, QuerierWrapper, Storage};
pub(crate) mod gas_coin;
pub(crate) mod history;
pub(crate) mod multitap;

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) env: Env,
}

pub(crate) struct StateContext<'a> {
    pub(crate) storage: &'a mut dyn Storage,
    pub(crate) response: ResponseBuilder,
}

impl<'a> State<'a> {
    pub(crate) fn new(deps: Deps<'a>, env: Env) -> (Self, &'a dyn Storage) {
        (
            State {
                api: deps.api,
                env,
                querier: deps.querier,
            },
            deps.storage,
        )
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
            },
            StateContext {
                storage: deps.storage,
                response: ResponseBuilder::new(contract_version),
            },
        ))
    }
}
