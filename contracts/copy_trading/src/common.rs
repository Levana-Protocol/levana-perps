use crate::{
    prelude::*,
    types::{
        CrankFeeConfig, DecQueuePosition, IncQueuePosition, MarketInfo, MarketLoaderStatus,
        OneLpTokenValue, OpenPositionsResp, PositionCollateral, State, TokenResp, Totals,
    },
};
use anyhow::{bail, Context, Result};
use perpswap::{
    contracts::{
        factory::entry::MarketsResp,
        market::{
            deferred_execution::{DeferredExecId, GetDeferredExecResp, ListDeferredExecsResp},
            entry::{
                ClosedPositionCursor, ClosedPositionsResp, LimitOrdersResp,
                PositionsQueryFeeApproach,
            },
            order::OrderId,
            position::{PositionId, PositionsResp},
        },
    },
    price::PricePoint,
};
use perpswap::{namespace::FACTORY_MARKET_LAST_ADDED, time::Timestamp};

pub(crate) const SIX_HOURS_IN_SECONDS: u64 = 6 * 60 * 60;

impl<'a> State<'a> {
    pub(crate) fn to_token(&self, token: &perpswap::token::Token) -> Result<Token> {
        let token = match token {
            perpswap::token::Token::Cw20 { addr, .. } => {
                let addr = addr.validate(self.api)?;
                Token::Cw20(addr)
            }
            perpswap::token::Token::Native { denom, .. } => Token::Native(denom.clone()),
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
                my_addr: env.contract.address.clone(),
                env,
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
                env: env.clone(),
            },
            deps.storage,
        ))
    }

    fn load_market_ids(&self, start_after: Option<MarketId>) -> Result<Vec<MarketId>> {
        let factory = &self.config.factory;
        let MarketsResp { markets } = self.querier.query_wasm_smart(
            factory,
            &perpswap::contracts::factory::entry::QueryMsg::Markets {
                start_after,
                limit: Some(30),
            },
        )?;
        Ok(markets)
    }

    pub(crate) fn batched_stored_market_info(&self, storage: &mut dyn Storage) -> Result<()> {
        let status = crate::state::MARKET_LOADER_STATUS
            .may_load(storage)?
            .unwrap_or_default();
        let start_after = match status {
            MarketLoaderStatus::NotStarted => None,
            MarketLoaderStatus::OnGoing { last_seen } => Some(last_seen),
            MarketLoaderStatus::Finished { last_seen } => {
                // This codepath will only reach when six hours have
                // exceeded and we want to check if the remote factory
                // contract has changed.
                let factory_market_last_added = self.raw_query_last_market_added()?;
                let last_seen = match factory_market_last_added {
                    Some(factory_market_last_added) => {
                        // This codepath will reach when factory added
                        // some market at some point of time.
                        let market_added_at =
                            crate::state::LAST_MARKET_ADD_CHECK.may_load(storage)?;
                        if let Some(market_added_at) = market_added_at {
                            if market_added_at < factory_market_last_added {
                                // Was the factory updated since we last
                                // loaded market in this contract ?
                                last_seen
                            } else {
                                crate::state::LAST_MARKET_ADD_CHECK
                                    .save(storage, &Timestamp::into(self.env.block.time.into()))?;
                                return Ok(());
                            }
                        } else {
                            bail!("Impossible case: LAST_MARKET_ADD_CHECK is not loaded in finished step")
                        }
                    }
                    None => {
                        // Remote factory contract doesn't have
                        // anything set. That means no market was
                        // newly added since we loaded it last
                        // time. We return early since we have nothing
                        // to query and store.
                        return Ok(());
                    }
                };
                Some(last_seen)
            }
        };
        let markets = self.load_market_ids(start_after.clone())?;
        if markets.is_empty() {
            crate::state::LAST_MARKET_ADD_CHECK
                .save(storage, &Timestamp::into(self.env.block.time.into()))?;
            if let Some(last_seen) = start_after {
                crate::state::MARKET_LOADER_STATUS
                    .save(storage, &MarketLoaderStatus::Finished { last_seen })?;
            }
            return Ok(());
        } else {
            let mut last_seen = None;
            for market in markets {
                let result = self.load_cache_market_info(storage, &market)?;
                last_seen = Some(result.id);
            }
            if let Some(last_seen) = last_seen {
                crate::state::MARKET_LOADER_STATUS
                    .save(storage, &MarketLoaderStatus::OnGoing { last_seen })?;
            }
        }
        crate::state::LAST_MARKET_ADD_CHECK
            .save(storage, &Timestamp::into(self.env.block.time.into()))?;
        Ok(())
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

        let perpswap::contracts::factory::entry::MarketInfoResponse {
            market_addr,
            position_token: _,
            liquidity_token_lp: _,
            liquidity_token_xlp: _,
        } = self
            .querier
            .query_wasm_smart(
                &self.config.factory,
                &perpswap::contracts::factory::entry::QueryMsg::MarketInfo {
                    market_id: market_id.clone(),
                },
            )
            .with_context(|| {
                format!(
                    "Unable to load market info for {market_id} from factory {}",
                    self.config.factory
                )
            })?;

        let status: perpswap::contracts::market::entry::StatusResp = self
            .querier
            .query_wasm_smart(
                &market_addr,
                &perpswap::contracts::market::entry::QueryMsg::Status { price: None },
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
            let token = self.to_token(&market.token)?;
            crate::state::MARKETS_TOKEN.save(storage, (token, market.id.clone()), &market)?;
        }
        Ok(market)
    }

    pub(crate) fn raw_query_last_market_added(&self) -> Result<Option<Timestamp>> {
        let contract = &self.config.factory;
        let key = FACTORY_MARKET_LAST_ADDED.as_bytes().to_vec();
        let result = self.querier.query_wasm_raw(contract, key)?;
        match result {
            Some(result) => {
                let time = cosmwasm_std::from_json(result.as_slice())?;
                Ok(Some(time))
            }
            None => Ok(None),
        }
    }

    pub(crate) fn get_first_full_token_info(
        &self,
        storage: &dyn Storage,
        token: &Token,
    ) -> Result<perpswap::token::Token> {
        let market = crate::state::MARKETS_TOKEN
            .prefix(token.clone())
            .range(storage, None, None, cosmwasm_std::Order::Ascending)
            .next();
        match market {
            Some(market_info) => {
                let (_, market_info) = market_info?;
                Ok(market_info.token)
            }
            None => Err(anyhow!("{token} not supported by factory")),
        }
    }

    pub(crate) fn load_market_ids_with_token(
        &self,
        storage: &dyn Storage,
        token: &Token,
        start_from: Option<MarketId>,
    ) -> Result<Vec<MarketInfo>> {
        let min = start_from.map(Bound::inclusive);
        let markets = crate::state::MARKETS_TOKEN.prefix(token.clone()).range(
            storage,
            min,
            None,
            Order::Ascending,
        );
        let mut result = vec![];
        for market in markets {
            let (_, market_info) = market?;
            result.push(market_info);
        }
        Ok(result)
    }

    /// Load position ID tokens belonging to this contract. Typically
    /// used to find all open positions. Has a default max limit of 10.
    pub(crate) fn query_tokens(
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
                nft_msg: perpswap::contracts::position_token::entry::QueryMsg::Tokens {
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

    /// Query for open positions
    pub(crate) fn query_positions(
        &self,
        market_addr: &Addr,
        position_ids: Vec<PositionId>,
    ) -> Result<OpenPositionsResp> {
        let PositionsResp {
            positions,
            pending_close,
            closed,
        } = self.querier.query_wasm_smart(
            market_addr,
            &MarketQueryMsg::Positions {
                position_ids,
                skip_calc_pending_fees: None,
                fees: Some(PositionsQueryFeeApproach::Accumulated),
                price: None,
            },
        )?;
        ensure!(
            pending_close.is_empty(),
            "pending_close is not empty in positions response"
        );
        ensure!(
            closed.is_empty(),
            "closed is not empty in positions response"
        );
        let start_after = positions.last().cloned().map(|item| item.id);
        Ok(OpenPositionsResp {
            positions,
            start_after,
        })
    }

    pub(crate) fn query_closed_position(
        &self,
        market_addr: &Addr,
        cursor: Option<ClosedPositionCursor>,
    ) -> Result<ClosedPositionsResp> {
        let copy_trading = self.my_addr.clone();
        let result = self.querier.query_wasm_smart(
            market_addr,
            &MarketQueryMsg::ClosedPositionHistory {
                owner: copy_trading.into(),
                cursor,
                limit: Some(15),
                order: Some(perpswap::storage::OrderInMessage::Ascending),
            },
        )?;
        Ok(result)
    }

    pub(crate) fn query_orders(
        &self,
        market_addr: &Addr,
        start_after: Option<OrderId>,
    ) -> Result<LimitOrdersResp> {
        let result = self.querier.query_wasm_smart(
            market_addr,
            &MarketQueryMsg::LimitOrders {
                owner: self.my_addr.as_ref().into(),
                start_after,
                limit: Some(15),
                order: Some(perpswap::storage::OrderInMessage::Ascending),
            },
        )?;
        Ok(result)
    }

    pub(crate) fn get_deferred_exec(
        &self,
        market_addr: &Addr,
        id: DeferredExecId,
    ) -> Result<GetDeferredExecResp> {
        let result = self
            .querier
            .query_wasm_smart(market_addr, &MarketQueryMsg::GetDeferredExec { id })?;
        Ok(result)
    }

    pub(crate) fn query_deferred_execs(
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

    pub(crate) fn query_spot_price(&self, market_addr: &Addr) -> Result<PricePoint> {
        let result = self
            .querier
            .query_wasm_smart(market_addr, &MarketQueryMsg::SpotPrice { timestamp: None })?;
        Ok(result)
    }

    fn raw_query_crank_fee(&self, market: &MarketInfo) -> Result<CrankFeeConfig> {
        let contract = market.addr.clone();
        let key = perpswap::namespace::CONFIG.as_bytes().to_vec();
        let result = self
            .querier
            .query_wasm_raw(contract, key)?
            .context("No Config found on market contract")?;
        let result = cosmwasm_std::from_json(result)?;
        Ok(result)
    }

    pub(crate) fn estimate_crank_fee(&self, market: &MarketInfo) -> Result<Collateral> {
        let crank_fee = self.raw_query_crank_fee(&market)?;
        let estimated_queue_size = 5u32;
        let fees = crank_fee
            .crank_fee_surcharge
            .checked_mul_dec(Decimal256::from_ratio(estimated_queue_size, 10u32))?;
        let fees = fees.checked_add(crank_fee.crank_fee_charged)?;
        let price = self.query_spot_price(&market.addr)?;
        Ok(price.usd_to_collateral(fees))
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
        token_value: &OneLpTokenValue,
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

/// Get queue element from Dec queue to process currently
pub(crate) fn get_current_processed_dec_queue_id(
    storage: &dyn Storage,
) -> Result<Option<(DecQueuePositionId, DecQueuePosition)>> {
    let queue_id = crate::state::LAST_PROCESSED_DEC_QUEUE_ID.may_load(storage)?;
    let queue_id = match queue_id {
        Some(queue_id) => queue_id.next(),
        None => DecQueuePositionId::new(0),
    };
    let queue_item = crate::state::COLLATERAL_DECREASE_QUEUE.may_load(storage, &queue_id)?;
    match queue_item {
        Some(queue_item) => Ok(Some((queue_id, queue_item))),
        None => Ok(None),
    }
}

/// Get queue element from Inc queue to process currently
pub(crate) fn get_current_processed_inc_queue_id(
    storage: &dyn Storage,
) -> Result<Option<(IncQueuePositionId, IncQueuePosition)>> {
    let queue_id = crate::state::LAST_PROCESSED_INC_QUEUE_ID.may_load(storage)?;
    let queue_id = match queue_id {
        Some(queue_id) => queue_id.next(),
        None => IncQueuePositionId::new(0),
    };
    let queue_item = crate::state::COLLATERAL_INCREASE_QUEUE.may_load(storage, &queue_id)?;
    match queue_item {
        Some(queue_item) => Ok(Some((queue_id, queue_item))),
        None => Ok(None),
    }
}

/// Get current queue element id that needs to be processed. Before
/// calling this function ensure that there is atleast one pending
/// element in the queue to be processed.
pub(crate) fn get_current_queue_element(storage: &dyn Storage) -> Result<QueuePositionId> {
    let inc_queue = get_current_processed_inc_queue_id(storage)?;
    match inc_queue {
        Some((queue_id, _)) => Ok(QueuePositionId::IncQueuePositionId(queue_id)),
        None => {
            let dec_queue = get_current_processed_dec_queue_id(storage)?;
            match dec_queue {
                Some((queue_id, _)) => Ok(QueuePositionId::DecQueuePositionId(queue_id)),
                None => bail!("No queue item found to process"),
            }
        }
    }
}
