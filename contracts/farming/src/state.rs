pub(crate) mod auth;
pub(crate) mod farming;
pub(crate) mod funds;
pub(crate) mod lockdrop;
pub(crate) mod market_info;
pub(crate) mod period;
pub(crate) mod status;

use crate::prelude::*;
use cosmwasm_std::{Api, Deps, DepsMut, Empty, Env, QuerierWrapper, Storage};

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) env: Env,
    pub(crate) market_info: MarketInfo,
    #[allow(dead_code)]
    pub(crate) querier: QuerierWrapper<'a, Empty>,
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
                querier: deps.querier,
                env,
                market_info: MarketInfo::load(deps.storage)?,
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
                market_info: MarketInfo::load(deps.storage)?,
            },
            StateContext {
                storage: deps.storage,
                response: ResponseBuilder::new(contract_version),
            },
        ))
    }
}
