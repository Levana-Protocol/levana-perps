use crate::state::*;
use cosmwasm_std::QueryResponse;
use msg::contracts::{
    factory::entry::{MarketInfoResponse, QueryMsg as FactoryQueryMsg},
    liquidity_token::entry::{
        ExecuteMsg as LiquidityTokenExecuteMsg, QueryMsg as LiquidityTokenQueryMsg,
    },
    market::entry::{ExecuteMsg as MarketExecuteMsg, QueryMsg as MarketQueryMsg},
};
use msg::prelude::*;

const MARKET_ID: Item<MarketId> = Item::new(namespace::MARKET_ID);

pub(crate) fn market_init(store: &mut dyn Storage, market_id: MarketId) -> Result<()> {
    MARKET_ID.save(store, &market_id)?;
    Ok(())
}

fn get_market_id(store: &dyn Storage) -> Result<MarketId> {
    MARKET_ID.load(store).map_err(|err| err.into())
}

impl State<'_> {
    pub(crate) fn market_addr(&self, store: &dyn Storage) -> Result<Addr> {
        let market_id = get_market_id(store)?;

        let resp: MarketInfoResponse = self.querier.query_wasm_smart(
            &self.factory_address,
            &FactoryQueryMsg::MarketInfo { market_id },
        )?;
        Ok(resp.market_addr)
    }

    pub(crate) fn market_query_liquidity_token(
        &self,
        store: &dyn Storage,
        msg: LiquidityTokenQueryMsg,
    ) -> Result<QueryResponse> {
        smart_query_no_parse(
            &self.querier,
            self.market_addr(store)?,
            &MarketQueryMsg::LiquidityTokenProxy {
                kind: get_kind(store)?,
                msg,
            },
        )
    }

    pub(crate) fn market_execute_liquidity_token(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        msg: LiquidityTokenExecuteMsg,
    ) -> Result<()> {
        ctx.response.add_execute_submessage_oneshot(
            self.market_addr(ctx.storage)?,
            &MarketExecuteMsg::LiquidityTokenProxy {
                sender: sender.into(),
                kind: get_kind(ctx.storage)?,
                msg,
            },
        )
    }
}
