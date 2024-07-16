use msg::contracts::cw20::entry::MarketingInfoResponse;

use crate::prelude::*;

impl<'a> State<'a> {
    pub(crate) fn load(
        api: &'a dyn Api,
        querier: QuerierWrapper<'a, Empty>,
        store: &dyn Storage,
    ) -> Result<Self> {
        Ok(State {
            config: crate::state::CONFIG
                .load(store)
                .context("Could not load config")?,
            api,
            querier,
        })
    }

    /// Returns true if loaded from the cache.
    fn load_market_info(
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
            token: status.collateral,
        };
        Ok((info, false))
    }
}

impl<'a> MarketState<'a> {
    pub(crate) fn load(deps: Deps<'a>, market_id: MarketId) -> Result<Self> {
        let state = State::load(deps.api, deps.querier, deps.storage)?;
        let (market, _) = state.load_market_info(deps.storage, &market_id)?;
        Ok(MarketState { state, market })
    }

    pub(crate) fn load_mut(
        deps: DepsMut<'a>,
        market_id: MarketId,
    ) -> Result<(Self, &mut dyn Storage)> {
        let state = State::load(deps.api, deps.querier, deps.storage)?;
        let (market, is_cached) = state.load_market_info(deps.storage, &market_id)?;
        if !is_cached {
            crate::state::MARKETS
                .save(deps.storage, &market.id, &market)
                .context("Could not save cached markets info")?;
        }
        Ok((MarketState { state, market }, deps.storage))
    }
}
