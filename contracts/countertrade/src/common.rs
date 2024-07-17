use crate::prelude::*;

impl<'a> State<'a> {
    pub(crate) fn load(deps: Deps<'a>) -> Result<(Self, &'a dyn Storage)> {
        let config = crate::state::CONFIG
            .load(deps.storage)
            .context("Could not load config")?;
        Ok((
            State {
                config,
                api: deps.api,
                querier: deps.querier,
            },
            deps.storage,
        ))
    }

    pub(crate) fn load_mut(deps: DepsMut<'a>) -> Result<(Self, &'a mut dyn Storage)> {
        let config = crate::state::CONFIG
            .load(deps.storage)
            .context("Could not load config")?;
        Ok((
            State {
                config,
                api: deps.api,
                querier: deps.querier,
            },
            deps.storage,
        ))
    }

    pub(crate) fn load_market_info(
        &self,
        store: &dyn Storage,
        market_id: &MarketId,
    ) -> Result<MarketInfo> {
        self.load_market_info_inner(store, market_id).map(|x| x.0)
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
