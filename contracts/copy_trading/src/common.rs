use crate::{
    prelude::*,
    types::{
        MarketInfo, OneLpTokenValue, OpenPositionsResp, PositionCollateral, State, TokenResp,
        Totals,
    },
};
use anyhow::{bail, Context, Result};
use msg::contracts::{
    factory::entry::MarketsResp,
    market::{
        deferred_execution::{DeferredExecId, ListDeferredExecsResp},
        entry::{LimitOrdersResp, PositionsQueryFeeApproach},
        order::OrderId,
        position::{PositionId, PositionsResp},
    },
};

impl<'a> State<'a> {
    pub(crate) fn to_token(&self, token: &msg::token::Token) -> Result<Token> {
        let token = match token {
            msg::token::Token::Cw20 { addr, .. } => {
                let addr = addr.validate(self.api)?;
                Token::Cw20(addr)
            }
            msg::token::Token::Native { denom, .. } => Token::Native(denom.clone()),
        };
        Ok(token)
    }

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

    pub(crate) fn load_mut(deps: DepsMut<'a>, env: &Env) -> Result<(Self, &'a mut dyn Storage)> {
        let config = crate::state::CONFIG
            .load(deps.storage)
            .context("Could not load config")?;
        Ok((
            State {
                config,
                api: deps.api,
                querier: deps.querier,
                my_addr: env.contract.address.clone(),
            },
            deps.storage,
        ))
    }

    pub(crate) fn load_all_market_ids(&self) -> Result<Vec<MarketId>> {
        let factory = &self.config.factory;
        let mut all_markets = vec![];
        let mut start_after = None;
        loop {
            let MarketsResp { mut markets } = self.querier.query_wasm_smart(
                factory,
                &msg::contracts::factory::entry::QueryMsg::Markets {
                    start_after,
                    limit: None,
                },
            )?;
            if markets.is_empty() {
                return Ok(all_markets);
            }
            start_after = markets.last().cloned();
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

    pub(crate) fn load_lp_token_value(
        &self,
        storage: &mut dyn Storage,
        token: &Token,
    ) -> Result<OneLpTokenValue> {
        let lp_token_value = crate::state::LP_TOKEN_VALUE.key(token).may_load(storage)?;
        let result = match lp_token_value {
            Some(lp_token_value) => lp_token_value.value,
            None => bail!("LP_TOKEN_VALUE not computed yet"),
        };
        Ok(result)
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

    pub(crate) fn get_full_token_info(
        &self,
        storage: &mut dyn Storage,
        token: &Token,
    ) -> Result<msg::token::Token> {
        let markets = self.load_all_market_ids()?;
        for market_id in markets {
            let market_info = self.load_cache_market_info(storage, &market_id)?;
            if token.is_same(&market_info.token) {
                return Ok(market_info.token);
            }
        }
        Err(anyhow!("{token} not supported by factory"))
    }

    pub(crate) fn load_market_ids_with_token(
        &self,
        storage: &mut dyn Storage,
        token: &Token,
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
    ) -> Result<OpenPositionsResp> {
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
        assert!(pending_close.is_empty());
        let start_after = positions.last().cloned().map(|item| item.id);
        Ok(OpenPositionsResp {
            positions,
            start_after,
        })
    }

    pub(crate) fn load_orders(
        &self,
        market_addr: &Addr,
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

    pub(crate) fn load_deferred_execs(
        &self,
        market_addr: &Addr,
        start_after: Option<DeferredExecId>,
        limit: Option<u32>,
    ) -> Result<ListDeferredExecsResp> {
        let result = self.querier.query_wasm_smart(
            market_addr,
            &MarketQueryMsg::ListDeferredExecs {
                addr: self.my_addr.as_ref().into(),
                start_after,
                limit,
            },
        )?;
        Ok(result)
    }
}

impl Totals {
    /// Convert an amount of shares into collateral.
    #[allow(dead_code)]
    pub(crate) fn shares_to_collateral(
        &self,
        shares: LpToken,
        pos: &PositionCollateral,
    ) -> Result<Collateral> {
        let total_collateral = self.collateral.checked_add(pos.0)?;
        let one_share_value = total_collateral
            .into_decimal256()
            .checked_div(self.shares.into_decimal256())?;
        let share_collateral = shares.into_decimal256().checked_mul(one_share_value)?;
        Ok(Collateral::from_decimal256(share_collateral))
    }

    /// Returns the newly minted share amount
    pub(crate) fn add_collateral(
        &mut self,
        funds: NonZero<Collateral>,
        token_value: OneLpTokenValue,
    ) -> Result<NonZero<LpToken>> {
        let new_shares = token_value.collateral_to_shares(funds)?;

        self.collateral = self.collateral.checked_add(funds.raw())?;
        self.shares = self.shares.checked_add(new_shares.raw())?;
        Ok(new_shares)
    }

    /// Returns the collateral removed from the pool
    #[allow(dead_code)]
    pub(crate) fn remove_collateral(
        &mut self,
        amount: NonZero<LpToken>,
        pos: &PositionCollateral,
    ) -> Result<Collateral> {
        let collateral = self.shares_to_collateral(amount.raw(), pos)?;
        ensure!(
            collateral <= self.collateral,
            "Insufficient collateral for withdrawal. Requested: {collateral}. Available: {}",
            self.collateral
        );
        ensure!(
            amount.raw() <= self.shares,
            "Insufficient shares for withdrawal. Requested: {amount}. Available: {}",
            self.shares
        );
        self.collateral = self.collateral.checked_sub(collateral)?;
        self.shares = self.shares.checked_sub(amount.raw())?;
        Ok(collateral)
    }
}

pub(crate) fn get_next_inc_queue_id(storage: &mut dyn Storage) -> Result<IncQueuePositionId> {
    let queue_id = crate::state::LAST_INSERTED_INC_QUEUE_ID
        .may_load(storage)
        .context("Could not load LAST_INSERTED_INC_QUEUE_ID")?;
    let queue_id = match queue_id {
        Some(queue_id) => queue_id.next(),
        None => IncQueuePositionId::new(0),
    };
    crate::state::LAST_INSERTED_INC_QUEUE_ID.save(storage, &queue_id)?;
    Ok(queue_id)
}

pub(crate) fn get_next_dec_queue_id(storage: &mut dyn Storage) -> Result<DecQueuePositionId> {
    let queue_id = crate::state::LAST_INSERTED_DEC_QUEUE_ID
        .may_load(storage)
        .context("Could not load LAST_INSERTED_DEC_QUEUE_ID")?;
    let queue_id = match queue_id {
        Some(queue_id) => queue_id.next(),
        None => DecQueuePositionId::new(0),
    };
    crate::state::LAST_INSERTED_DEC_QUEUE_ID.save(storage, &queue_id)?;
    Ok(queue_id)
}
