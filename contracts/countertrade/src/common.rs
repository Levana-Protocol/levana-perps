use crate::prelude::*;

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) config: Config,
}

impl<'a> State<'a> {
    pub(crate) fn load(api: &'a dyn Api, store: &dyn Storage) -> Result<Self> {
        todo!()
    }

    fn load_token_info(
        &self,
        store: &dyn Storage,
        market_id: &MarketId,
    ) -> Result<msg::token::Token> {
        todo!()
    }
}

pub(crate) struct MarketState<'a> {
    pub(crate) state: State<'a>,
    pub(crate) market_id: MarketId,
    pub(crate) token: msg::token::Token,
}

impl<'a> MarketState<'a> {
    pub(crate) fn load(api: &'a dyn Api, store: &dyn Storage, market_id: MarketId) -> Result<Self> {
        let state = State::load(api, store)?;
        let token = state.load_token_info(store, &market_id)?;
        Ok(MarketState {
            state,
            market_id,
            token,
        })
    }
}
