use crate::state::*;
use cosmwasm_std::QueryResponse;
use cw_storage_plus::Item;
use msg::contracts::{
    factory::entry::{MarketInfoResponse, QueryMsg as FactoryQueryMsg},
    market::entry::{ExecuteMsg as MarketExecuteMsg, QueryMsg as MarketQueryMsg},
    position_token::entry::{ExecuteMsg as NftExecuteMsg, QueryMsg as NftQueryMsg},
};
use msg::prelude::*;

const MARKET_ID: Item<MarketId> = Item::new(namespace::MARKET_ID);

impl State<'_> {
    pub(crate) fn market_id(&self, store: &dyn Storage) -> Result<MarketId> {
        MARKET_ID.load(store).map_err(|err| err.into())
    }

    pub(crate) fn market_addr(&self, store: &dyn Storage) -> Result<Addr> {
        let market_id = self.market_id(store)?;

        let resp: MarketInfoResponse = self.querier.query_wasm_smart(
            &self.factory_address,
            &FactoryQueryMsg::MarketInfo { market_id },
        )?;
        Ok(resp.market_addr)
    }

    pub(crate) fn market_query_nft(
        &self,
        store: &dyn Storage,
        nft_msg: NftQueryMsg,
    ) -> Result<QueryResponse> {
        smart_query_no_parse(
            &self.querier,
            self.market_addr(store)?,
            &MarketQueryMsg::NftProxy { nft_msg },
        )
    }

    pub(crate) fn market_init(&self, ctx: &mut StateContext, market_id: MarketId) -> Result<()> {
        MARKET_ID.save(ctx.storage, &market_id)?;
        Ok(())
    }

    pub(crate) fn market_execute_nft(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        msg: NftExecuteMsg,
    ) -> Result<()> {
        ctx.response.add_execute_submessage_oneshot(
            self.market_addr(ctx.storage)?,
            &MarketExecuteMsg::NftProxy {
                sender: sender.into(),
                msg,
            },
        )
    }
}
