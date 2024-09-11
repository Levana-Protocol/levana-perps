use crate::{
    prelude::*,
    types::{MarketInfo, PositionInfo, State, TokenResp},
};
use anyhow::{Context, Result};
use msg::contracts::{
    factory::entry::MarketsResp,
    market::{
        entry::{LimitOrdersResp, PositionsQueryFeeApproach},
        order::OrderId,
        position::{PositionId, PositionQueryResponse, PositionsResp},
    },
};

impl<'a> State<'a> {
    pub(crate) fn load(deps: Deps<'a>, env: Env) -> Result<(Self, &'a dyn Storage)> {
        let config = crate::state::CONFIG
            .load(deps.storage)
            .context("Could not load config")?;
        Ok((
            State {
                config,
                api: deps.api,
                querier: deps.querier,
                my_addr: env.contract.address,
            },
            deps.storage,
        ))
    }

    pub(crate) fn load_mut(deps: DepsMut<'a>, env: Env) -> Result<(Self, &'a mut dyn Storage)> {
        let config = crate::state::CONFIG
            .load(deps.storage)
            .context("Could not load config")?;
        Ok((
            State {
                config,
                api: deps.api,
                querier: deps.querier,
                my_addr: env.contract.address,
            },
            deps.storage,
        ))
    }

    pub(crate) fn load_all_market_ids(&self) -> Result<Vec<MarketId>> {
        let factory = &self.config.factory;
        let mut all_markets = vec![];
        loop {
            let mut start_after = None;
            let MarketsResp { mut markets } = self.querier.query_wasm_smart(
                factory.clone(),
                &msg::contracts::factory::entry::QueryMsg::Markets {
                    start_after,
                    limit: None,
                },
            )?;
            if markets.is_empty() {
                return Ok(all_markets);
            }
            start_after = markets.last().clone().cloned();
            all_markets.append(&mut markets);
        }
    }

    /// Returns true if loaded from the cache.
    fn load_market_info_inner(
        &self,
        store: &dyn Storage,
        market_id: &MarketId,
    ) -> Result<(MarketInfo, bool)> {
        if let Some(info) = crate::state::MARKETS
            .may_load(store, market_id)
            .context("Could not load cached market info")?
        {
            return Ok((info, true));
        }

        let msg::contracts::factory::entry::MarketInfoResponse {
            market_addr,
            position_token: _,
            liquidity_token_lp: _,
            liquidity_token_xlp: _,
        } = self
            .querier
            .query_wasm_smart(
                &self.config.factory,
                &msg::contracts::factory::entry::QueryMsg::MarketInfo {
                    market_id: market_id.clone(),
                },
            )
            .with_context(|| {
                format!(
                    "Unable to load market info for {market_id} from factory {}",
                    self.config.factory
                )
            })?;

        let status: msg::contracts::market::entry::StatusResp = self
            .querier
            .query_wasm_smart(
                &market_addr,
                &msg::contracts::market::entry::QueryMsg::Status { price: None },
            )
            .with_context(|| format!("Unable to load market status from contract {market_addr}"))?;

        let info = MarketInfo {
            id: status.market_id,
            addr: market_addr,
            token: status.collateral,
        };
        Ok((info, false))
    }

    pub(crate) fn load_cache_market_info(
        &self,
        storage: &mut dyn Storage,
        market_id: &MarketId,
    ) -> Result<MarketInfo> {
        let (market, is_cached) = self.load_market_info_inner(storage, market_id)?;
        if !is_cached {
            crate::state::MARKETS
                .save(storage, &market.id, &market)
                .context("Could not save cached markets info")?;
        }
        Ok(market)
    }

    pub(crate) fn load_market_ids_with_token(
        &self,
        storage: &mut dyn Storage,
        token: Token,
    ) -> Result<Vec<MarketInfo>> {
        let markets = self.load_all_market_ids()?;
        let mut result = vec![];
        for market_id in markets {
            let market_info = self.load_cache_market_info(storage, &market_id)?;
            if token.is_same(&market_info.token) {
                result.push(market_info);
            }
        }
        Ok(result)
    }

    /// Load position ID tokens belonging to this contract. Typically
    /// used to find all open positions.
    pub(crate) fn load_tokens(
        &self,
        market_addr: &Addr,
        start_after: Option<String>,
    ) -> Result<TokenResp> {
        #[derive(serde::Deserialize)]
        struct Resp {
            tokens: Vec<PositionId>,
        }
        let Resp { tokens } = self.querier.query_wasm_smart(
            market_addr,
            &MarketQueryMsg::NftProxy {
                nft_msg: msg::contracts::position_token::entry::QueryMsg::Tokens {
                    owner: self.my_addr.as_ref().into(),
                    start_after,
                    limit: None,
                },
            },
        )?;
        let start_after = tokens.last().map(|item| item.to_string());
        Ok(TokenResp {
            tokens,
            start_after,
        })
    }

    /// Load open positions
    pub(crate) fn load_positions(
        &self,
        market_addr: &Addr,
        position_ids: Vec<PositionId>,
    ) -> Result<Vec<PositionQueryResponse>> {
        let PositionsResp {
            positions,
            pending_close,
            closed: _,
        } = self.querier.query_wasm_smart(
            market_addr,
            &MarketQueryMsg::Positions {
                position_ids,
                skip_calc_pending_fees: None,
                fees: Some(PositionsQueryFeeApproach::Accumulated),
                price: None,
            },
        )?;
        // todo: Change this to Error
        assert!(pending_close.len() == 0);
        Ok(positions)
    }

    pub(crate) fn load_orders(
        &self,
        market_addr: Addr,
        start_after: Option<OrderId>,
    ) -> Result<LimitOrdersResp> {
        let result = self.querier.query_wasm_smart(
            market_addr,
            &MarketQueryMsg::LimitOrders {
                owner: self.my_addr.as_ref().into(),
                start_after,
                limit: None,
                order: None,
            },
        )?;
        Ok(result)
    }
}

impl MarketInfo {
    fn process_open_positions(
        &mut self,
        state: &State,
        market: &MarketInfo,
        unprocessed_open_positions: Vec<PositionId>,
    ) -> Result<()> {
        // todo: this needs to be split
        let resp: PositionsResp = state.querier.query_wasm_smart(
            &market.addr,
            &MarketQueryMsg::Positions {
                position_ids: unprocessed_open_positions,
                skip_calc_pending_fees: None,
                fees: Some(PositionsQueryFeeApproach::Accumulated),
                price: None,
            },
        )?;
        let open_positions = resp.positions.into_iter().map(|position| PositionInfo {
            id: position.id,
            active_collateral: position.active_collateral,
            pnl_collateral: position.pnl_collateral,
            pnl_usd: position.pnl_usd,
        });
        // todo: Push resp.closed into pending_closed_positions todo: Push
        // resp.pending_close into pending_close position. The reason
        // being you have do another smart query using the same api which will give pending_closed anyway.
        todo!()
    }
}
