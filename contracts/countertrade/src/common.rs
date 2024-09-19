use msg::contracts::market::{
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
                nft_msg: msg::contracts::position_token::entry::QueryMsg::Tokens {
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
    use std::str::FromStr;

    use cosmwasm_std::{Addr, Decimal256};
    use msg::{
        contracts::market::{
            config::{Config, MaxLiquidity},
            entry::{Fees, StatusResp},
            liquidity::LiquidityStats,
        },
        token::Token,
    };
    use shared::{
        number::{Collateral, Notional, Number, UnsignedDecimal},
        storage::{MarketId, MarketType},
    };

    use crate::{work::smart_search, PositionsInfo, Totals};

    #[test]
    fn regression_perp_4062() {
        let totals = Totals {
            collateral: "0.000000000000005108".parse().unwrap(),
            shares: "0.000000000000005108".parse().unwrap(),
            last_closed: None,
            deferred_exec: None,
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
        };
        let my_shares = totals.shares;
        let my_collateral = totals
            .shares_to_collateral(my_shares, &PositionsInfo::NoPositions)
            .unwrap();
        assert!(totals.collateral.approx_eq(my_collateral));
    }

    #[test]
    fn regression_perp_4098() {
        let status = StatusResp {
            market_id: MarketId::new("INJ", "USDC", MarketType::CollateralIsQuote),
            base: "INJ".to_owned(),
            quote: "USDC".to_owned(),
            market_type: MarketType::CollateralIsQuote,
            collateral: Token::Native {
                denom: "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4"
                    .to_owned(),
                decimal_places: 6,
            },
            config: Config {
                trading_fee_notional_size: "0.001".parse().unwrap(),
                trading_fee_counter_collateral: "0.001".parse().unwrap(),
                crank_execs: 7,
                max_leverage: "10".parse().unwrap(),
                funding_rate_sensitivity: "2".parse().unwrap(),
                funding_rate_max_annualized: "0.9".parse().unwrap(),
                borrow_fee_rate_min_annualized: "0.08".parse().unwrap(),
                borrow_fee_rate_max_annualized: "0.6".parse().unwrap(),
                carry_leverage: "5".parse().unwrap(),
                mute_events: false,
                liquifunding_delay_seconds: 86400,
                protocol_tax: "0.3".parse().unwrap(),
                unstake_period_seconds: 3888000,
                target_utilization: "0.5".parse().unwrap(),
                borrow_fee_sensitivity: "0.3".parse().unwrap(),
                max_xlp_rewards_multiplier: "2".parse().unwrap(),
                min_xlp_rewards_multiplier: "1".parse().unwrap(),
                delta_neutrality_fee_sensitivity: "1000000".parse().unwrap(),
                delta_neutrality_fee_cap: "0.005".parse().unwrap(),
                delta_neutrality_fee_tax: "0.25".parse().unwrap(),
                crank_fee_charged: "0.02".parse().unwrap(),
                crank_fee_surcharge: "0.01".parse().unwrap(),
                crank_fee_reward: "0.018".parse().unwrap(),
                minimum_deposit_usd: "5".parse().unwrap(),
                liquifunding_delay_fuzz_seconds: 3600,
                max_liquidity: MaxLiquidity::Unlimited {},
                disable_position_nft_exec: false,
                liquidity_cooldown_seconds: 86400,
                exposure_margin_ratio: "0.005".parse().unwrap(),
                referral_reward_ratio: "0.05".parse().unwrap(),
                spot_price: msg::contracts::market::spot_price::SpotPriceConfig::Manual {
                    admin: Addr::unchecked("admin"),
                },
                _unused1: None,
                _unused2: None,
                _unused3: None,
                _unused4: None,
            },
            liquidity: LiquidityStats::default(),
            next_crank: None,
            last_crank_completed: None,
            next_deferred_execution: None,
            newest_deferred_execution: None,
            next_liquifunding: None,
            deferred_execution_items: 0,
            last_processed_deferred_exec_id: None,
            borrow_fee: "0.08".parse().unwrap(),
            borrow_fee_lp: "0.041384622370943865".parse().unwrap(),
            borrow_fee_xlp: "0.038615377629056135".parse().unwrap(),
            long_funding: Number::from_str("0.142902225796709546").unwrap(),
            short_funding: Number::from_str("-0.164894655512925805").unwrap(),
            long_notional: Notional::from_str("68.116739816667650139").unwrap(),
            short_notional: Notional::from_str("59.031832799784842895").unwrap(),
            long_usd: "1318.484645076373203003".parse().unwrap(),
            short_usd: "1142.634913630835365286".parse().unwrap(),
            instant_delta_neutrality_fee_value: "0.000009084907016882".parse().unwrap(),
            delta_neutrality_fee_fund: "0.000801830464723314".parse().unwrap(),
            fees: Fees {
                wallets: "1926.179500580295401611".parse().unwrap(),
                protocol: "39.611714585701583531".parse().unwrap(),
                crank: "0.016".parse().unwrap(),
                referral: "0.329530977743406304".parse().unwrap(),
            },
        };
        let target_funding = Number::from(Decimal256::from_ratio(40u32, 100u32));
        let long_notional = Notional::from_str("68.116739816667650139").unwrap();
        let short_notional = Notional::from_str("59.031832799784842895").unwrap();
        let long_notional = long_notional
            .checked_sub("13.677338545852927893".parse().unwrap())
            .unwrap();
        let notional = crate::work::smart_search(
            long_notional,
            short_notional,
            target_funding,
            &status,
            150,
            0,
        )
        .unwrap();
        println!("{notional}");
        assert_eq!(2, 3);
    }
}
