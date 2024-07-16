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
    pub(crate) fn load_market_info(
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
impl Totals {
    /// Convert an amount of shares into collateral.
    pub(crate) fn shares_to_collateral(
        &self,
        shares: LpToken,
        pos: &PositionsInfo,
    ) -> Result<Collateral> {
        let collateral = self.collateral.checked_add(pos.active_collateral())?;
        Ok(Collateral::from_decimal256(
            shares
                .into_decimal256()
                .checked_mul(collateral.into_decimal256())?
                .checked_div(self.shares.into_decimal256())?,
        ))
    }

    /// Returns the newly minted share amount
    pub(crate) fn add_collateral(
        &mut self,
        funds: NonZero<Collateral>,
        pos: &PositionsInfo,
    ) -> Result<NonZero<LpToken>> {
        let collateral = self.collateral.checked_add(pos.active_collateral())?;
        let new_shares = if collateral.is_zero() && self.shares.is_zero() {
            NonZero::new(LpToken::from_decimal256(funds.into_decimal256()))
                .expect("Impossible: NonZero to NonZero produced a 0")
        } else if collateral.is_zero() || self.shares.is_zero() {
            bail!("Invalid collateral/shares totals: {self:?}");
        } else {
            let new_shares = LpToken::from_decimal256(
                funds
                    .into_decimal256()
                    .checked_mul(self.shares.into_decimal256())?
                    .checked_div(self.collateral.into_decimal256())?,
            );
            NonZero::new(new_shares).context("new_shares ended up 0")?
        };
        self.collateral = self.collateral.checked_add(funds.raw())?;
        self.shares = self.shares.checked_add(new_shares.raw())?;
        Ok(new_shares)
    }

    /// Returns the collateral removed from the pool
    pub(crate) fn remove_collateral(
        &mut self,
        amount: NonZero<LpToken>,
        pos: &PositionsInfo,
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

impl PositionsInfo {
    pub(crate) fn load() -> Self {
        // FIXME
        PositionsInfo {}
    }

    pub(crate) fn active_collateral(&self) -> Collateral {
        Collateral::zero() // FIXME
    }
}
