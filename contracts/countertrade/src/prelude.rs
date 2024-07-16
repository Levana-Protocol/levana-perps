pub(crate) use crate::types::{MarketInfo, MarketState, State};
pub(crate) use cosmwasm_std::{
    entry_point, from_json, to_json_binary, Addr, Api, Binary, Coin, Decimal256, Deps, DepsMut,
    Empty, Env, MessageInfo, QuerierWrapper, Response, Storage, Uint128,
};
pub(crate) use cw_storage_plus::{Item, Map};
pub(crate) use msg::contracts::countertrade::*;
pub(crate) use shared::{
    attr_map,
    storage::{Collateral, LpToken, MarketId, NonZero},
};

pub(crate) trait ResultExt<T>: Sized {
    fn context(self, ctx: impl Into<String>) -> Result<T, Error> {
        self.with_context(|| ctx.into())
    }
    fn with_context(self, ctx: impl FnOnce() -> String) -> Result<T, Error>;
}

impl<T, E: std::error::Error + 'static> ResultExt<T> for Result<T, E> {
    fn with_context(self, ctx: impl FnOnce() -> String) -> Result<T, Error> {
        self.map_err(|e| Error::Context {
            source: Box::new(e),
            context: ctx(),
        })
    }
}
