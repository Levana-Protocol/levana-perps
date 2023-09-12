// re-exporting
pub(crate) mod config;
pub(crate) mod crank;
pub mod data_series;
pub(crate) mod delta_neutrality_fee;
pub(crate) mod fees;
pub(crate) mod funding;
pub(crate) mod history;
pub(crate) mod liquidity;
pub(crate) mod meta;
pub(crate) mod order;
pub(crate) mod position;
#[cfg(feature = "sanity")]
pub(crate) mod sanity;
pub(crate) mod shutdown;
pub(crate) mod spot_price;
pub(crate) mod stale;
pub(crate) mod status;
pub(crate) mod token;

use crate::prelude::*;
use cosmwasm_std::{Addr, Api, Deps, DepsMut, Empty, Env, QuerierWrapper, Response, Storage};
use cw2::ContractVersion;
use cw_storage_plus::Item;
use msg::token::Token;
use once_cell::unsync::OnceCell;
use std::collections::HashMap;

use self::config::load_config;
use self::liquidity::LiquidityCache;

/// The factory address - here because this interface is part of StateExt
const FACTORY_ADDR: Item<Addr> = Item::new(namespace::FACTORY_ADDR);

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) env: Env,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) factory_address: Addr,
    pub(crate) contract_version: ContractVersion,
    pub(crate) config: Config,

    /// Cache variables
    spot_price_cache: OnceCell<PricePoint>,
    market_id_cache: OnceCell<MarketId>,
    token_cache: OnceCell<Token>,
    pub(crate) liquidity_cache: LiquidityCache,
}

pub(crate) struct StateContext<'a> {
    pub(crate) storage: &'a mut dyn Storage,
    /// Hidden so that it's not possible to generate a Response without dealing with [StateContext::fund_transfers]
    response: ResponseBuilder,
    /// Funds to be transferred to other addresses
    pub(crate) fund_transfers: HashMap<Addr, NonZero<Collateral>>,
}

impl<'a> State<'a> {
    fn new_inner(
        api: &'a dyn Api,
        querier: QuerierWrapper<'a, Empty>,
        env: Env,
        store: &dyn Storage,
    ) -> Result<Self> {
        let factory_address = FACTORY_ADDR.load(store)?;
        let contract_version = get_contract_version(store)?;
        let config = load_config(store)?;
        Ok(State {
            api,
            env,
            querier,
            factory_address,
            contract_version,
            config,
            spot_price_cache: OnceCell::new(),
            market_id_cache: OnceCell::new(),
            token_cache: OnceCell::new(),
            liquidity_cache: LiquidityCache::default(),
        })
    }

    pub(crate) fn new(deps: Deps<'a>, env: Env) -> Result<(Self, &dyn Storage)> {
        let state = State::new_inner(deps.api, deps.querier, env, deps.storage)?;
        Ok((state, deps.storage))
    }

    pub(crate) fn now(&self) -> Timestamp {
        self.env.block.time.into()
    }

    pub(crate) fn assert_auth(&self, addr: &Addr, check: AuthCheck) -> Result<()> {
        assert_auth(&self.factory_address, &self.querier, addr, check)
    }
}

impl<'a> StateContext<'a> {
    pub(crate) fn new(deps: DepsMut<'a>, env: Env) -> Result<(State<'a>, Self)> {
        let state = State::new_inner(deps.api, deps.querier, env, deps.storage)?;
        let ctx = StateContext {
            storage: deps.storage,
            response: if state.config.mute_events {
                ResponseBuilder::new_mute_events()
            } else {
                ResponseBuilder::new(state.contract_version.clone())
            },
            fund_transfers: HashMap::new(),
        };
        Ok((state, ctx))
    }

    pub(crate) fn response_mut(&mut self) -> &mut ResponseBuilder {
        &mut self.response
    }

    pub(crate) fn into_response(mut self, state: &State) -> Result<Response> {
        let token = state.get_token(self.storage)?;
        for (addr, amount) in self.fund_transfers {
            if let Some(msg) = token.into_transfer_msg(&addr, amount)? {
                self.response.add_message(msg);
            }
        }

        Ok(self.response.into_response())
    }
}

pub(crate) fn set_factory_addr(store: &mut dyn Storage, factory_addr: &Addr) -> Result<()> {
    FACTORY_ADDR.save(store, factory_addr)?;

    Ok(())
}
