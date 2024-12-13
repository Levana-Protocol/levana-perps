use perpswap::contracts::market::{
    entry::PositionsQueryFeeApproach,
    position::{PositionId, PositionsResp},
};

use crate::prelude::*;

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

    pub(crate) fn load_market_info(&self, store: &dyn Storage) -> Result<MarketInfo> {
        self.load_market_info_inner(store).map(|x| x.0)
    }

    /// Returns true if loaded from the cache.
    fn load_market_info_inner(&self, store: &dyn Storage) -> Result<(MarketInfo, bool)> {
        if let Some(info) = crate::state::MARKETS
            .may_load(store)
            .context("Could not load cached market info")?
        {
            return Ok((info, true));
        }

        let status: perpswap::contracts::market::entry::StatusResp = self
            .querier
            .query_wasm_smart(
                &self.config.market,
                &perpswap::contracts::market::entry::QueryMsg::Status { price: None },
            )
            .with_context(|| {
                format!(
                    "Unable to load market status from contract {}",
                    self.config.market.clone()
                )
            })?;

        let info = MarketInfo {
            id: status.market_id,
            addr: self.config.market.clone(),
            token: status.collateral,
        };
        Ok((info, false))
    }

    // todo: Use only this
    pub(crate) fn load_cache_market_info(&self, storage: &mut dyn Storage) -> Result<MarketInfo> {
        let (market, is_cached) = self.load_market_info_inner(storage)?;
        if !is_cached {
            crate::state::MARKETS
                .save(storage, &market)
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
        let total_collateral = self.collateral.checked_add(pos.active_collateral()?)?;
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
        pos: &PositionsInfo,
    ) -> Result<NonZero<LpToken>> {
        let collateral = self.collateral.checked_add(pos.active_collateral()?)?;
        let new_shares =
            if (collateral.is_zero() && self.shares.is_zero()) || self.collateral.is_zero() {
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
    pub(crate) fn load(state: &State, market: &MarketInfo) -> Result<Self> {
        #[derive(serde::Deserialize)]
        struct Resp {
            tokens: Vec<PositionId>,
        }
        let Resp { tokens } = state.querier.query_wasm_smart(
            &market.addr,
            &MarketQueryMsg::NftProxy {
                nft_msg: perpswap::contracts::position_token::entry::QueryMsg::Tokens {
                    owner: state.my_addr.as_ref().into(),
                    start_after: None,
                    limit: None,
                },
            },
        )?;

        match tokens.first() {
            None => Ok(Self::NoPositions),
            Some(pos_id) => {
                if tokens.len() > 1 {
                    Ok(Self::TooManyPositions { to_close: *pos_id })
                } else {
                    let PositionsResp {
                        mut positions,
                        pending_close: _,
                        closed: _,
                    } = state.querier.query_wasm_smart(
                        &market.addr,
                        &MarketQueryMsg::Positions {
                            position_ids: vec![*pos_id],
                            skip_calc_pending_fees: None,
                            fees: Some(PositionsQueryFeeApproach::Accumulated),
                            price: None,
                        },
                    )?;
                    match positions.pop() {
                        Some(pos) => Ok(Self::OnePosition { pos:Box::new(pos)  }),
                        None => Err(anyhow!("Our open position {pos_id} in {} is in an unhealthy state, waiting for cranks", market.id)),
                    }
                }
            }
        }
    }

    pub(crate) fn active_collateral(&self) -> Result<Collateral> {
        match self {
            PositionsInfo::TooManyPositions { to_close: _ } => bail!(
                "Invalid state detected, multiple positions open. Perform work to close those."
            ),
            PositionsInfo::NoPositions => Ok(Collateral::zero()),
            PositionsInfo::OnePosition { pos } => Ok(pos.active_collateral.raw()),
        }
    }
}

#[cfg(test)]
mod tests {
    use perpswap::number::{Collateral, UnsignedDecimal};

    use crate::{PositionsInfo, Totals};

    #[test]
    fn regression_perp_4062() {
        let totals = Totals {
            collateral: "0.000000000000005108".parse().unwrap(),
            shares: "0.000000000000005108".parse().unwrap(),
            last_closed: None,
            deferred_exec: None,
            deferred_collateral: None,
        };
        let my_shares = totals.shares;
        let my_collateral = totals
            .shares_to_collateral(my_shares, &PositionsInfo::NoPositions)
            .unwrap();
        assert_ne!(my_collateral, Collateral::zero());
        assert!(my_collateral.approx_eq(totals.collateral));

        let totals = Totals {
            collateral: "9999999999999999".parse().unwrap(),
            shares: "0.000000000000005108".parse().unwrap(),
            last_closed: None,
            deferred_exec: None,
            deferred_collateral: None,
        };
        let my_shares = totals.shares;
        let my_collateral = totals
            .shares_to_collateral(my_shares, &PositionsInfo::NoPositions)
            .unwrap();
        assert!(totals.collateral.approx_eq(my_collateral));

        let totals = Totals {
            collateral: "0.000000000000005108".parse().unwrap(),
            shares: "9999999999999999".parse().unwrap(),
            last_closed: None,
            deferred_exec: None,
            deferred_collateral: None,
        };
        let my_shares = totals.shares;
        let my_collateral = totals
            .shares_to_collateral(my_shares, &PositionsInfo::NoPositions)
            .unwrap();
        assert!(totals.collateral.approx_eq(my_collateral));

        let totals = Totals {
            collateral: "999999999999999999".parse().unwrap(),
            shares: "999999999999999999".parse().unwrap(),
            last_closed: None,
            deferred_exec: None,
            deferred_collateral: None,
        };
        let my_shares = totals.shares;
        let my_collateral = totals
            .shares_to_collateral(my_shares, &PositionsInfo::NoPositions)
            .unwrap();
        assert!(totals.collateral.approx_eq(my_collateral));
    }
}
