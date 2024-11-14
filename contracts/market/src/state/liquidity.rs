mod cw20;
mod stats;

use crate::state::*;
use anyhow::Context;
use cosmwasm_std::Order;
use cw_storage_plus::Map;
use perpswap::contracts::liquidity_token::LiquidityTokenKind;
use perpswap::contracts::market::config::MaxLiquidity;
use perpswap::contracts::market::entry::{LiquidityCooldown, LpInfoResp, UnstakingStatus};
use perpswap::contracts::market::liquidity::events::{
    DeltaNeutralityRatioEvent, DepositEvent, LockEvent, UnlockEvent, WithdrawEvent,
};
use perpswap::contracts::market::liquidity::events::{LiquidityPoolSizeEvent, LockUpdateEvent};
use perpswap::contracts::market::liquidity::LiquidityStats;
use perpswap::prelude::*;
use serde::{Deserialize, Serialize};
pub(crate) use stats::*;
use std::cmp::Ordering;

use super::funding::LpAndXlp;

/// Tracks how much yield is allocated per token at a given timestamp
///
/// The key is a monotonically increasing index.
pub(super) const YIELD_PER_TIME_PER_TOKEN: Map<u64, YieldPerToken> =
    Map::new(namespace::YIELD_PER_TIME_PER_TOKEN);

pub(crate) fn yield_init(store: &mut dyn Storage) -> Result<()> {
    YIELD_PER_TIME_PER_TOKEN
        .save(
            store,
            0,
            &YieldPerToken {
                lp: Collateral::zero(),
                xlp: Collateral::zero(),
            },
        )
        .map_err(|e| e.into())
}

/// Stores how much yield is allocated per LP and xLP token as prefix sums.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct YieldPerToken {
    /// Prefix sum for LP tokens
    lp: Collateral,
    /// Prefix sum for xLP tokens
    xlp: Collateral,
}

/// Liquidity information per individual liquidity provider
///
/// When the liquidity provider is not in the process of unstaking xLP into LP,
/// the fields `lp` and `xlp` below are straightforward: they represent the
/// total number of LP and xLP tokens held by this provider, respectively.
///
/// However, during an unstaking process, it's a bit different. When a wallet
/// begins unstaking, we immediately deduct the total amount to be unstaked from
/// the `xlp` field and set it as [UnstakingInfo::xlp_amount]. We also update
/// the protocol-wide totals of LP and xLP to reflect this change.
///
/// In order to calculate the instantaneous true LP and xLP balances during unstaking, we have the following rules:
///
/// * Let `uncollected` be the total amount of LP that could be collected by hasn't been yet
///
/// * `real_lp = self.lp + uncollected`
///
/// * `real_xlp = self.xlp + self.unstaking.xlp_amount - self.unstaking.collected - uncollected`
///
/// The purpose of all this is to allow a linear unstaking process from xLP and
/// LP, and to ensure that during that unstaking process the wallet receives
/// yields as if all unstaking xLP is already LP. Without that behavior,
/// liquidity providers could preemptively unstake all their xLP and keep higher
/// rewards without a lockup period.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct LiquidityStatsByAddr {
    pub(crate) lp: LpToken,
    pub(crate) xlp: LpToken,
    /// Key in the [YIELD_PER_TIME_PER_TOKEN] that we last accrued from.
    pub(crate) last_accrue_key: u64,
    pub(crate) lp_accrued_yield: Collateral,
    pub(crate) xlp_accrued_yield: Collateral,
    pub(crate) crank_rewards: Collateral,
    #[serde(default)]
    pub(crate) referrer_rewards: Collateral,
    pub(crate) unstaking: Option<UnstakingXlp>,
    /// When the liquidity cooldown period ends, if active.
    pub(crate) cooldown_ends: Option<Timestamp>,
}

impl LiquidityStatsByAddr {
    fn new(state: &State, store: &dyn Storage) -> Result<Self> {
        let last_accrue_key = state.latest_yield_per_token(store)?.0;
        Ok(Self {
            lp: LpToken::zero(),
            xlp: LpToken::zero(),
            last_accrue_key,
            lp_accrued_yield: Collateral::zero(),
            xlp_accrued_yield: Collateral::zero(),
            crank_rewards: Collateral::zero(),
            referrer_rewards: Collateral::zero(),
            unstaking: None,
            cooldown_ends: None,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        #[cfg(debug_assertions)]
        if let Some(unstaking) = &self.unstaking {
            // If we ever collect the entire amount, unstaking should be removed.
            debug_assert!(unstaking.collected < unstaking.xlp_amount.raw());
        }
        self.lp.is_zero()
            && self.xlp.is_zero()
            && self.unstaking.is_none()
            && self.lp_accrued_yield.is_zero()
            && self.xlp_accrued_yield.is_zero()
            && self.crank_rewards.is_zero()
            && self.referrer_rewards.is_zero()
    }

    fn total_yield(&self) -> Result<Collateral> {
        Ok(self
            .lp_accrued_yield
            .checked_add(self.xlp_accrued_yield)?
            .checked_add(self.crank_rewards)?
            .checked_add(self.referrer_rewards)?)
    }
}

/// Tracks current status of unstaking xLP tokens per wallet.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub(crate) struct UnstakingXlp {
    /// Amount of xLP tokens to be unstaked
    pub(crate) xlp_amount: NonZero<LpToken>,
    /// Total amount of LP tokens already collected
    pub(crate) collected: LpToken,
    /// When did we start the unstaking process?
    unstake_started: Timestamp,
    /// How long will the unstaking process take?
    ///
    /// This comes from the config. However, we store it here in case the config
    /// changes while we're in the middle of unstaking.
    unstake_duration: Duration,
    /// What is the timestamp of the last time we collected?
    last_collected: Timestamp,
}

impl State<'_> {
    pub(crate) fn liquidity_token_addr(
        &self,
        store: &dyn Storage,
        kind: LiquidityTokenKind,
    ) -> Result<Addr> {
        load_external_map(
            &self.querier,
            &self.factory_address,
            match kind {
                LiquidityTokenKind::Lp => namespace::LP_ADDRS,
                LiquidityTokenKind::Xlp => namespace::XLP_ADDRS,
            },
            self.market_id(store)?,
        )
    }
}

impl State<'_> {
    pub(crate) fn load_liquidity_stats_addr(
        &self,
        store: &dyn Storage,
        lp_addr: &Addr,
    ) -> Result<LiquidityStatsByAddr> {
        self.load_liquidity_stats_addr_may(store, lp_addr)?
            .with_context(|| format!("No liquidity stats found for {lp_addr}"))
    }

    /// Stores new yield to be used during future calculations
    pub(crate) fn liquidity_process_new_yield(
        &self,
        store: &dyn Storage,
        new_yield: LpAndXlp,
    ) -> Result<LiquidityNewYieldToProcess> {
        let stats = self.load_liquidity_stats(store)?;
        let lp_yield_per_token = match NonZero::new(stats.total_lp.into_decimal256()) {
            None => {
                debug_assert_eq!(new_yield.lp, Collateral::zero());
                Collateral::zero()
            }
            Some(total_lp) => new_yield.lp.div_non_zero_dec(total_lp),
        };
        let xlp_yield_per_token = match NonZero::new(stats.total_xlp.into_decimal256()) {
            None => {
                debug_assert_eq!(new_yield.xlp, Collateral::zero());
                Collateral::zero()
            }
            Some(total_xlp) => new_yield.xlp.div_non_zero_dec(total_xlp),
        };

        let (last_index, last_yield) = self.latest_yield_per_token(store)?;

        let new_yield = YieldPerToken {
            lp: last_yield.lp.checked_add(lp_yield_per_token)?,
            xlp: last_yield.xlp.checked_add(xlp_yield_per_token)?,
        };

        let next_index = last_index + 1;

        Ok(LiquidityNewYieldToProcess {
            next_index,
            new_yield,
        })
    }

    /// Get the latest key and value from [YIELD_PER_TIME_PER_TOKEN].
    fn latest_yield_per_token(&self, store: &dyn Storage) -> Result<(u64, YieldPerToken)> {
        YIELD_PER_TIME_PER_TOKEN
            .range(store, None, None, Order::Descending)
            .next()
            .expect("YIELD_PER_TIME_PER_TOKEN cannot be empty")
            .map_err(|e| e.into())
    }

    /// Adds the specified amount to the unlocked liquidity fund and allocates the corresponding
    /// amount of LP shares to the liquidity provider
    ///
    pub(crate) fn liquidity_deposit(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        amount: NonZero<Collateral>,
        stake_to_xlp: bool,
    ) -> Result<()> {
        let lp_shares = self.liquidity_deposit_inner(ctx, lp_addr, amount, !stake_to_xlp)?;
        self.lp_history_add_deposit(ctx, lp_addr, lp_shares, amount, stake_to_xlp)?;
        if stake_to_xlp {
            self.liquidity_stake_lp(ctx, lp_addr, Some(lp_shares))?;
        }

        Ok(())
    }

    /// Reinvest any pending yield into LP or xLP tokens
    ///
    /// Errors if no pending yield is available
    pub(crate) fn reinvest_yield(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        amount: Option<NonZero<Collateral>>,
        stake_to_xlp: bool,
    ) -> Result<()> {
        let yield_to_reinvest = self.liquidity_claim_yield(ctx, lp_addr, false)?;

        let yield_to_reinvest = match amount {
            None => yield_to_reinvest,
            Some(amount) => {
                let remainder = match yield_to_reinvest.raw().checked_sub(amount.raw()) {
                    Ok(remainder) => remainder,
                    Err(_) => {
                        #[derive(serde::Serialize)]
                        struct Data {
                            requested_to_reinvest: NonZero<Collateral>,
                            available_yield: NonZero<Collateral>,
                        }
                        perp_bail_data!(
                            ErrorId::InsufficientForReinvest,
                            ErrorDomain::Market,
                            Data {
                                requested_to_reinvest: amount,
                                available_yield: yield_to_reinvest
                            },
                            "Requested to reinvest {amount}, but only have {yield_to_reinvest}"
                        );
                    }
                };
                if let Some(remainder) = NonZero::new(remainder) {
                    self.add_token_transfer_msg(ctx, lp_addr, remainder)?;
                    self.lp_history_add_claim_yield(ctx, lp_addr, remainder)?;
                }
                amount
            }
        };

        let lp_shares = self.liquidity_deposit_inner(ctx, lp_addr, yield_to_reinvest, false)?;
        if stake_to_xlp {
            self.liquidity_stake_lp(ctx, lp_addr, Some(lp_shares))?;
        }

        self.lp_history_add_reinvest_yield(
            ctx,
            lp_addr,
            lp_shares,
            yield_to_reinvest,
            stake_to_xlp,
        )?;

        Ok(())
    }

    pub(crate) fn add_delta_neutrality_ratio_event(
        &self,
        ctx: &mut StateContext,
        stats: &LiquidityStats,
        price_point: &PricePoint,
    ) -> Result<()> {
        let total_liquidity = stats.total_collateral()?;

        // Use the market type internal to the protocol
        let long_interest_protocol = self.open_long_interest(ctx.storage)?;
        let short_interest_protocol = self.open_short_interest(ctx.storage)?;
        let market_type = self.market_type(ctx.storage)?;
        let (long_interest, short_interest) = match market_type {
            MarketType::CollateralIsQuote => (long_interest_protocol, short_interest_protocol),
            MarketType::CollateralIsBase => (short_interest_protocol, long_interest_protocol),
        };

        let net_notional = long_interest
            .into_signed()
            .checked_sub(short_interest.into_signed())?;
        let delta_neutrality_ratio = net_notional
            .map(|x| price_point.notional_to_collateral(x))
            .into_number()
            .checked_div(total_liquidity.into_number())
            .ok()
            .unwrap_or_default();

        ctx.response_mut().add_event(DeltaNeutralityRatioEvent {
            total_liquidity,
            long_interest,
            short_interest,
            net_notional,
            price_notional: price_point.price_notional,
            delta_neutrality_ratio,
        });

        Ok(())
    }

    /// Add events which are emitted whenever the pool size changes.
    fn add_pool_size_change_events(
        &self,
        ctx: &mut StateContext,
        liquidity_stats: &LiquidityStats,
        price: &PricePoint,
    ) -> Result<()> {
        ctx.response_mut()
            .add_event(LiquidityPoolSizeEvent::from_stats(liquidity_stats, price)?);

        self.add_delta_neutrality_ratio_event(ctx, liquidity_stats, price)
    }

    /// Returns the number of LP shares minted.
    fn liquidity_deposit_inner(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        amount: NonZero<Collateral>,
        start_cooldown: bool,
    ) -> Result<NonZero<LpToken>> {
        let mut liquidity_stats = self.load_liquidity_stats(ctx.storage)?;
        self.ensure_max_liquidity(ctx, amount, &liquidity_stats)?;

        // Handle yield

        self.perform_lp_book_keeping(ctx, lp_addr)?;

        // Update liquidity and calculate shares

        let new_shares = liquidity_stats.collateral_to_lp(amount)?;
        liquidity_stats.total_lp = (liquidity_stats.total_lp + new_shares.raw())?;
        liquidity_stats.unlocked = (liquidity_stats.unlocked + amount.raw())?;

        self.save_liquidity_stats(ctx.storage, &liquidity_stats)?;
        let price = self.current_spot_price(ctx.storage)?;
        self.add_pool_size_change_events(ctx, &liquidity_stats, &price)?;

        // Update shares

        let mut stats = self.load_liquidity_stats_addr_default(ctx.storage, lp_addr)?;
        stats.lp = stats.lp.checked_add(new_shares.raw())?;

        if start_cooldown {
            stats.cooldown_ends = Some(
                self.now()
                    .plus_seconds(self.config.liquidity_cooldown_seconds.into()),
            );
        }

        self.save_liquidity_stats_addr(ctx.storage, lp_addr, &stats)?;

        let amount_usd = price.collateral_to_usd_non_zero(amount);

        ctx.response_mut().add_event(DepositEvent {
            amount,
            amount_usd,
            shares: new_shares,
        });

        Ok(new_shares)
    }

    /// Removes either the specified amount of LP shares or all of them if `None` is specified.
    pub(crate) fn liquidity_withdraw(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        lp_amount: Option<NonZero<LpToken>>,
    ) -> Result<()> {
        // Handle yield

        self.perform_lp_book_keeping(ctx, lp_addr)?;

        // Update liquidity provider's shares

        let mut addr_stats = self.load_liquidity_stats_addr(ctx.storage, lp_addr)?;
        self.ensure_liquidity_cooldown(&addr_stats)?;
        let old_lp = NonZero::new(addr_stats.lp).with_context(|| {
            format!("unable to withdraw, no liquidity deposited for {}", lp_addr)
        })?;

        let shares_to_withdraw = match lp_amount {
            None => old_lp,
            Some(lp_amount) => {
                if lp_amount > old_lp {
                    return Err(MarketError::WithdrawTooMuch {
                        requested: lp_amount,
                        available: old_lp,
                    }
                    .into());
                }

                lp_amount
            }
        };

        addr_stats.lp = addr_stats
            .lp
            .checked_sub(shares_to_withdraw.raw())
            .context("Tried to withdraw more LP tokens than you have")?;
        self.save_liquidity_stats_addr(ctx.storage, lp_addr, &addr_stats)?;

        // Update total and calculate returns

        let mut liquidity_stats = self.load_liquidity_stats(ctx.storage)?;

        let total_collateral = liquidity_stats
            .locked
            .checked_add(liquidity_stats.unlocked)?;
        let liquidity_to_return = liquidity_stats.lp_to_collateral_non_zero(shares_to_withdraw)?;

        let long_interest_protocol = self.open_long_interest(ctx.storage)?;
        let short_interest_protocol = self.open_short_interest(ctx.storage)?;
        let net_notional =
            (long_interest_protocol.into_signed() - short_interest_protocol.into_signed())?;
        let price = self.current_spot_price(ctx.storage)?;
        let min_unlocked_liquidity = self.min_unlocked_liquidity(net_notional, &price)?;
        if (liquidity_to_return.raw() + min_unlocked_liquidity)? > liquidity_stats.unlocked {
            return Err(MarketError::InsufficientLiquidityForWithdrawal {
                requested_lp: shares_to_withdraw,
                requested_collateral: liquidity_to_return,
                unlocked: (liquidity_stats.unlocked.into_signed()
                    - min_unlocked_liquidity.into_signed())?
                .max(Collateral::zero().into_signed())
                .abs_unsigned(),
            }
            .into());
        }

        debug_assert!(total_collateral >= liquidity_to_return.raw());
        liquidity_stats.total_lp = liquidity_stats
            .total_lp
            .checked_sub(shares_to_withdraw.raw())?;
        liquidity_stats.unlocked = liquidity_stats
            .unlocked
            .checked_sub(liquidity_to_return.raw())?;

        // PERP-2487: rounding errors can leave a little bit of "dust" in collateral
        // we need to zero it out here if there's no liquidity tokens left
        if liquidity_stats.total_tokens()?.is_zero() {
            // sanity check, it really should just be dust, at most
            anyhow::ensure!(
                liquidity_stats
                    .total_collateral()?
                    .approx_eq(Collateral::zero()),
                "liquidity_withdraw: no lp tokens left, but collateral is not zero"
            );
            liquidity_stats = LiquidityStats::default();
        }

        self.save_liquidity_stats(ctx.storage, &liquidity_stats)?;
        self.add_pool_size_change_events(ctx, &liquidity_stats, &price)?;

        // Transfer funds to LP

        self.add_token_transfer_msg(ctx, lp_addr, liquidity_to_return)?;

        self.lp_history_add_withdraw(ctx, lp_addr, shares_to_withdraw, liquidity_to_return)?;

        ctx.response_mut().add_event(WithdrawEvent {
            burned_shares: shares_to_withdraw,
            withdrawn_funds: liquidity_to_return,
            withdrawn_funds_usd: price.collateral_to_usd_non_zero(liquidity_to_return),
        });

        Ok(())
    }

    /// Stake a wallet's LP tokens into xLP
    pub(crate) fn liquidity_stake_lp(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        lp_amount: Option<NonZero<LpToken>>,
    ) -> Result<()> {
        self.perform_lp_book_keeping(ctx, lp_addr)?;

        // Determine amounts to transfer
        let mut addr_stats = self.load_liquidity_stats_addr(ctx.storage, lp_addr)?;
        let old_lp = NonZero::new(addr_stats.lp).context("Cannot stake LP, no LP tokens found")?;
        let lp_amount = lp_amount.unwrap_or(old_lp);
        let mut stats = self.load_liquidity_stats(ctx.storage)?;

        // Update for the individual lp_addr
        addr_stats.lp = match old_lp.raw().checked_sub(lp_amount.raw()).ok() {
            None => {
                return Err(anyhow!(
                    "unable to stake LP, attempted amount: {lp_amount}, available LP: {old_lp}"
                ))
            }
            Some(new_lp) => new_lp,
        };
        addr_stats.xlp = addr_stats.xlp.checked_add(lp_amount.raw())?;
        self.save_liquidity_stats_addr(ctx.storage, lp_addr, &addr_stats)?;

        // Update the protocol stats
        stats.total_lp = stats.total_lp.checked_sub(lp_amount.raw())?;
        stats.total_xlp = stats.total_xlp.checked_add(lp_amount.raw())?;

        self.save_liquidity_stats(ctx.storage, &stats)?;

        Ok(())
    }

    /// Begin the unstaking process from xLP to LP
    pub(crate) fn liquidity_unstake_xlp(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        xlp_amount: Option<NonZero<LpToken>>,
    ) -> Result<()> {
        let mut addr_stats = self.liquidity_stop_unstaking_xlp(ctx, lp_addr, false, false)?;
        debug_assert_eq!(addr_stats.unstaking, None);

        let owned_xlp = NonZero::new(addr_stats.xlp)
            .with_context(|| format!("Wallet {lp_addr} does not have any xLP tokens to unstake"))?;
        let xlp_amount = match xlp_amount {
            None => owned_xlp,
            Some(xlp_amount) => {
                anyhow::ensure!(xlp_amount <= owned_xlp, "Insufficient xLP tokens for unstaking. Wanted to unstake: {xlp_amount}. Currently holding: {owned_xlp}");
                xlp_amount
            }
        };
        addr_stats.unstaking = Some(UnstakingXlp {
            xlp_amount,
            collected: LpToken::zero(),
            unstake_started: self.now(),
            unstake_duration: Duration::from_seconds(self.config.unstake_period_seconds.into()),
            last_collected: self.now(),
        });
        addr_stats.xlp = addr_stats.xlp.checked_sub(xlp_amount.raw())?;
        self.save_liquidity_stats_addr(ctx.storage, lp_addr, &addr_stats)?;

        // Immediately convert the totals to treat the unstaking xLP as LP for rewards purposes
        let mut stats = self.load_liquidity_stats(ctx.storage)?;
        stats.total_lp = stats.total_lp.checked_add(xlp_amount.raw())?;
        stats.total_xlp = stats.total_xlp.checked_sub(xlp_amount.raw())?;
        self.save_liquidity_stats(ctx.storage, &stats)?;

        // for historical events, need to express it as collateral
        let xlp_collateral_value = stats.lp_to_collateral_non_zero(xlp_amount)?;

        self.lp_history_add_unstake_xlp(ctx, lp_addr, xlp_amount, xlp_collateral_value)?;

        Ok(())
    }

    /// Terminates the unstaking process for the specified LP
    pub(crate) fn liquidity_stop_unstaking_xlp(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        fail_on_no_unstaking: bool,
        save_new_stats: bool,
    ) -> Result<LiquidityStatsByAddr> {
        self.perform_lp_book_keeping(ctx, lp_addr)?;
        let mut addr_stats = self.load_liquidity_stats_addr(ctx.storage, lp_addr)?;

        let old_unstaking = addr_stats.unstaking.take();

        match old_unstaking {
            Some(old_unstaking) => {
                let xlp_to_restore = old_unstaking
                    .xlp_amount
                    .raw()
                    .checked_sub(old_unstaking.collected)?;

                if !xlp_to_restore.is_zero() {
                    let mut stats = self.load_liquidity_stats(ctx.storage)?;
                    stats.total_lp = stats.total_lp.checked_sub(xlp_to_restore)?;
                    stats.total_xlp = stats.total_xlp.checked_add(xlp_to_restore)?;
                    self.save_liquidity_stats(ctx.storage, &stats)?;

                    addr_stats.xlp = addr_stats.xlp.checked_add(xlp_to_restore)?;
                }
            }
            None => {
                if fail_on_no_unstaking {
                    anyhow::bail!("Cannot stop unstaking, not currently unstaking xLP")
                }
            }
        }

        if save_new_stats {
            self.save_liquidity_stats_addr(ctx.storage, lp_addr, &addr_stats)?;
        }

        Ok(addr_stats)
    }

    /// Minimum amount of liquidity needed to allow a carry leverage trade that balances the net notional.
    pub(crate) fn min_unlocked_liquidity(
        &self,
        net_notional: Signed<Notional>,
        price: &PricePoint,
    ) -> Result<Collateral> {
        let net_notional_in_collateral = price.notional_to_collateral(net_notional.abs_unsigned());
        let counter_collateral = net_notional_in_collateral.div_non_zero_dec(
            NonZero::new(self.config.carry_leverage)
                .context("Carry leverage of 0 configuration error")?,
        );
        Ok(counter_collateral)
    }

    /// Sends all accrued yield to specified LP
    ///
    /// `send_to_wallet` indicates whether the resulting collateral should be
    /// sent to the wallet. If the protocol will continue to do something with
    /// that collateral, such as reinvesting, you can use `false`.
    ///
    /// Returns the amount of collateral claimed.
    pub(crate) fn liquidity_claim_yield(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        send_to_wallet: bool,
    ) -> Result<NonZero<Collateral>> {
        self.perform_lp_book_keeping(ctx, lp_addr)?;
        let mut addr_stats = self.load_liquidity_stats_addr(ctx.storage, lp_addr)?;
        let total_yield = addr_stats.total_yield()?;
        let total_yield = match NonZero::new(total_yield) {
            Some(total_yield) => total_yield,
            None => perp_bail!(
                ErrorId::NoYieldToClaim,
                ErrorDomain::Market,
                "liquidity_claim_yield: total yield is 0"
            ),
        };

        addr_stats.lp_accrued_yield = Collateral::zero();
        addr_stats.xlp_accrued_yield = Collateral::zero();
        addr_stats.crank_rewards = Collateral::zero();
        addr_stats.referrer_rewards = Collateral::zero();
        self.save_liquidity_stats_addr(ctx.storage, lp_addr, &addr_stats)?;

        self.register_lp_claimed_yield(ctx, total_yield)?;
        if send_to_wallet {
            self.add_token_transfer_msg(ctx, lp_addr, total_yield)?;
            self.lp_history_add_claim_yield(ctx, lp_addr, total_yield)?;
        }

        Ok(total_yield)
    }

    /// Performs book keeping on any pending values for a liquidity provider.
    ///
    /// Liquidity providers have multiple tasks that can occur over a period of
    /// time, such as receiving LP rewards or unstaking xLP into LP. This
    /// function performs all of that book keeping, guaranteeing that the
    /// liquidity provider has no outstanding values. This is a prerequisite for
    /// many actions within LP handling. For example, any time LP tokens are
    /// transferred or liquidity is locked, we need to update this book keeping.
    fn perform_lp_book_keeping(&self, ctx: &mut StateContext, lp_addr: &Addr) -> Result<()> {
        self.update_accrued_yield(ctx, lp_addr)?;
        self.collect_unstaked_lp(ctx, lp_addr)?;

        Ok(())
    }

    /// Allocates accrued yield to the specified LP
    fn update_accrued_yield(&self, ctx: &mut StateContext, lp_addr: &Addr) -> Result<()> {
        let mut addr_stats = self.load_liquidity_stats_addr_default(ctx.storage, lp_addr)?;

        if let Some((latest_index, accrued)) =
            self.calculate_accrued_yield(ctx.storage, &addr_stats)?
        {
            addr_stats.last_accrue_key = latest_index;
            addr_stats.lp_accrued_yield = addr_stats.lp_accrued_yield.checked_add(accrued.lp)?;
            addr_stats.xlp_accrued_yield = addr_stats.xlp_accrued_yield.checked_add(accrued.xlp)?;

            self.save_liquidity_stats_addr(ctx.storage, lp_addr, &addr_stats)?;
        }

        Ok(())
    }

    /// Add the given amount of funds to the crank rewards for a wallet.
    pub(crate) fn add_lp_crank_rewards(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        let mut addr_stats = self.load_liquidity_stats_addr_default(ctx.storage, lp_addr)?;
        addr_stats.crank_rewards = addr_stats.crank_rewards.checked_add(amount.raw())?;
        self.save_liquidity_stats_addr(ctx.storage, lp_addr, &addr_stats)
    }

    /// Calculates accrued yield for the specified LP since the last withdrawal.
    /// Will return [Number::ZERO] if there are no shares for the specified LP.
    ///
    /// Returns the index in the [YIELD_PER_TIME_PER_TOKEN] `Map` that we collected until.
    fn calculate_accrued_yield(
        &self,
        store: &dyn Storage,
        addr_stats: &LiquidityStatsByAddr,
    ) -> Result<Option<(u64, LpAndXlp)>> {
        let (end_index, end_yield) = self.latest_yield_per_token(store)?;
        debug_assert!(end_index >= addr_stats.last_accrue_key);
        if end_index == addr_stats.last_accrue_key {
            return Ok(None);
        }

        let start_yield = YIELD_PER_TIME_PER_TOKEN.load(store, addr_stats.last_accrue_key)?;

        let lp_amount = match &addr_stats.unstaking {
            Some(unstaking) => {
                // If we're in the middle of unstaking, treat the entire pending
                // unstake amount as LP for the purposes of rewards.
                addr_stats.lp.checked_add(
                    unstaking
                        .xlp_amount
                        .raw()
                        .checked_sub(unstaking.collected)?,
                )?
            }
            None => addr_stats.lp,
        };

        let lp_yield_per_token_sum = end_yield.lp.checked_sub(start_yield.lp)?;
        let lp_accrued = lp_yield_per_token_sum.checked_mul_dec(lp_amount.into_decimal256())?;
        let xlp_yield_per_token_sum = end_yield.xlp.checked_sub(start_yield.xlp)?;
        let xlp_accrued =
            xlp_yield_per_token_sum.checked_mul_dec(addr_stats.xlp.into_decimal256())?;
        Ok(Some((
            end_index,
            LpAndXlp {
                lp: lp_accrued,
                xlp: xlp_accrued,
            },
        )))
    }

    /// Returns the amount of LP that has unstaked and can now be collected after an xLP unstaking
    /// process has begun
    pub(crate) fn calculate_unstaked_lp(&self, unstaking_info: &UnstakingXlp) -> Result<LpToken> {
        let elapsed_since_start = self.now().checked_sub(
            unstaking_info.unstake_started,
            "calculate_unstaked_lp: elapsed_since_start",
        )?;

        let amount = if elapsed_since_start >= unstaking_info.unstake_duration {
            unstaking_info.xlp_amount.into_number() - unstaking_info.collected.into_number()
        } else {
            let elapsed_since_last_collected = (self.now().checked_sub(
                unstaking_info.last_collected,
                "calculate_unstaked_lp, elapsed_since_last_collected",
            )?)
            .as_nanos();
            let elapsed_ratio: Number = Number::from(elapsed_since_last_collected)
                .checked_div(unstaking_info.unstake_duration.as_nanos().into())?;
            elapsed_ratio.checked_mul(unstaking_info.xlp_amount.into_number())
        }?;

        debug_assert!(
            unstaking_info.collected.into_number().checked_add(amount)?
                <= unstaking_info.xlp_amount.into_number()
        );

        LpToken::try_from_number(amount).context("calculate_unstaked_lp: amount is negative")
    }

    /// Collect any LP tokens that have been unstaked from xLP.
    ///
    /// Returns a bool indicating whether there was any LP collected.
    pub(crate) fn collect_unstaked_lp(
        &self,
        ctx: &mut StateContext,
        lp_addr: &Addr,
    ) -> Result<bool> {
        // Somewhat wasteful, but first check if there's any unstaking info and return early otherwise.
        // We don't save these values, because the update method calls below will change what's contained.
        if self
            .load_liquidity_stats_addr_may(ctx.storage, lp_addr)?
            .and_then(|x| x.unstaking)
            .is_none()
        {
            return Ok(false);
        }

        self.update_accrued_yield(ctx, lp_addr)?;

        let mut addr_stats = self.load_liquidity_stats_addr(ctx.storage, lp_addr)?;
        let mut unstaking_info = addr_stats
            .unstaking
            .context("Withtin collect_unstaked_lp, unstaking is None")?;

        let unstaked_lp = self.calculate_unstaked_lp(&unstaking_info)?;

        // Return early if there's no unstaked LP. This can happen if this fn is called twice
        // in the same block
        let unstaked_lp = match NonZero::new(unstaked_lp) {
            None => return Ok(false),
            Some(unstaked_lp) => unstaked_lp,
        };

        unstaking_info.collected = (unstaking_info.collected + unstaked_lp.raw())?;
        unstaking_info.last_collected = self.now();
        match unstaking_info
            .collected
            .cmp(&unstaking_info.xlp_amount.raw())
        {
            Ordering::Less => addr_stats.unstaking = Some(unstaking_info),
            Ordering::Equal => addr_stats.unstaking = None,
            Ordering::Greater => anyhow::bail!(
                "unable to collect LP for {}, {} is more than was unstaked, {}",
                lp_addr,
                unstaking_info.collected,
                unstaking_info.xlp_amount
            ),
        };

        addr_stats.lp = addr_stats.lp.checked_add(unstaked_lp.raw())?;

        self.save_liquidity_stats_addr(ctx.storage, lp_addr, &addr_stats)?;

        // the pool size has _not_ changed here, so do not emit the event

        let stats = self.load_liquidity_stats(ctx.storage)?;
        self.lp_history_add_collect_lp(
            ctx,
            lp_addr,
            unstaked_lp,
            stats.lp_to_collateral_non_zero(unstaked_lp)?,
        )?;

        Ok(true)
    }

    pub(crate) fn lp_info(&self, store: &dyn Storage, lp_addr: &Addr) -> Result<LpInfoResp> {
        let stats = self.load_liquidity_stats(store)?;

        let addr_stats = self.load_liquidity_stats_addr_default(store, lp_addr)?;
        let liquidity_cooldown = self.get_liquidity_cooldown(&addr_stats)?;

        let accrued = self
            .calculate_accrued_yield(store, &addr_stats)?
            .map_or_else(LpAndXlp::zero, |x| x.1);
        let available_yield_lp = addr_stats.lp_accrued_yield.checked_add(accrued.lp)?;
        let available_yield_xlp = addr_stats.xlp_accrued_yield.checked_add(accrued.xlp)?;
        let available_yield = available_yield_lp
            .checked_add(available_yield_xlp)?
            .checked_add(addr_stats.crank_rewards)?
            .checked_add(addr_stats.referrer_rewards)?;

        let (lp_amount, xlp_amount, unstaking) = match addr_stats.unstaking {
            None => (addr_stats.lp, addr_stats.xlp, None),
            Some(unstaking_info) => {
                let unstaked_lp = self.calculate_unstaked_lp(&unstaking_info)?;
                (
                    addr_stats.lp.checked_add(unstaked_lp)?,
                    addr_stats
                        .xlp
                        .checked_add(unstaking_info.xlp_amount.raw())?
                        .checked_sub(unstaking_info.collected)?
                        .checked_sub(unstaked_lp)?,
                    Some(UnstakingStatus {
                        start: unstaking_info.unstake_started,
                        end: unstaking_info.unstake_started + unstaking_info.unstake_duration,
                        xlp_unstaking: unstaking_info.xlp_amount,
                        xlp_unstaking_collateral: stats
                            .lp_to_collateral(unstaking_info.xlp_amount.raw())?,
                        collected: unstaking_info.collected,
                        available: unstaked_lp,
                        pending: unstaking_info
                            .xlp_amount
                            .raw()
                            .checked_sub(unstaked_lp)?
                            .checked_sub(unstaking_info.collected)?,
                    }),
                )
            }
        };

        // Handle the degenerate case where all liquidity has been drained from
        // the pool. In such as case: we reset all balances to 0, except for the
        // available yield.
        let (lp_amount, xlp_amount, unstaking) = if stats.total_collateral()?.is_zero() {
            (LpToken::zero(), LpToken::zero(), None)
        } else {
            (lp_amount, xlp_amount, unstaking)
        };

        let history = self.lp_history_get_summary(store, lp_addr)?;

        Ok(LpInfoResp {
            lp_amount,
            lp_collateral: stats.lp_to_collateral(lp_amount)?,
            xlp_amount,
            xlp_collateral: stats.lp_to_collateral(xlp_amount)?,
            available_yield,
            available_yield_lp,
            available_yield_xlp,
            available_crank_rewards: addr_stats.crank_rewards,
            available_referrer_rewards: addr_stats.referrer_rewards,
            unstaking,
            history,
            liquidity_cooldown,
        })
    }

    /// Ensure that we have not exceeded max liquidity.
    fn ensure_max_liquidity(
        &self,
        ctx: &mut StateContext,
        deposit: NonZero<Collateral>,
        stats: &LiquidityStats,
    ) -> Result<()> {
        let max = match self.config.max_liquidity {
            MaxLiquidity::Unlimited {} => return Ok(()),
            MaxLiquidity::Usd { amount } => amount.raw(),
        };
        let price = self.current_spot_price(ctx.storage)?;
        let deposit = price.collateral_to_usd(deposit.raw());
        let current = stats.total_collateral()?;
        let current = price.collateral_to_usd(current);

        let new_total = current.checked_add(deposit)?;
        if new_total > max {
            Err(MarketError::MaxLiquidity {
                price_collateral_in_usd: price.price_usd,
                current,
                deposit,
                max,
            }
            .into_anyhow())
        } else {
            Ok(())
        }
    }

    /// Check if we're in a cooldown period and, if so, return an error.
    pub(crate) fn ensure_liquidity_cooldown(&self, stats: &LiquidityStatsByAddr) -> Result<()> {
        match self.get_liquidity_cooldown(stats)? {
            Some(LiquidityCooldown { at, seconds }) => Err(MarketError::LiquidityCooldown {
                ends_at: at,
                seconds_remaining: seconds,
            }
            .into_anyhow()),
            None => Ok(()),
        }
    }

    /// Get the liquidity cooldown stats for a provider.
    ///
    /// Returning [None] means that no period is currently active.
    fn get_liquidity_cooldown(
        &self,
        stats: &LiquidityStatsByAddr,
    ) -> Result<Option<LiquidityCooldown>> {
        let ends = match stats.cooldown_ends {
            Some(ends) => ends,
            None => return Ok(None),
        };
        if ends <= self.now() {
            return Ok(None);
        }
        Ok(Some(LiquidityCooldown {
            at: ends,
            seconds: ends
                .checked_sub(self.now(), "get_liquidity_cooldown")?
                .as_nanos()
                / 1_000_000_000,
        }))
    }
}

// Helper struct for liquidity unlock
#[must_use]
pub(crate) struct LiquidityUnlock {
    pub(crate) amount: NonZero<Collateral>,
    pub(crate) price: PricePoint,
    pub(crate) stats: LiquidityStats,
}

impl LiquidityUnlock {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        amount: NonZero<Collateral>,
        price: PricePoint,
        // optional to allow chaining liquidity updates in validation, before applying
        stats: Option<LiquidityStats>,
    ) -> Result<Self> {
        let mut stats = stats.unwrap_or(state.load_liquidity_stats(store)?);
        if amount.raw() > stats.locked {
            Err(MarketError::InsufficientLiquidityForUnlock {
                requested: amount,
                total_locked: stats.locked,
            }
            .into_anyhow())
        } else {
            stats.locked = stats.locked.checked_sub(amount.raw())?;
            stats.unlocked = stats.unlocked.checked_add(amount.raw())?;
            Ok(Self {
                stats,
                amount,
                price,
            })
        }
    }

    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<()> {
        let Self {
            amount,
            stats,
            price,
        } = self;

        state.save_liquidity_stats(ctx.storage, &stats)?;

        // Technically the total pool size has not changed
        // but the event consists of both locked and unlocked parts
        // so emit the event for now
        state.add_pool_size_change_events(ctx, &stats, &price)?;

        ctx.response_mut().add_event(UnlockEvent { amount });

        Ok(())
    }
}

// Helper struct for liquidity lock
#[must_use]
pub(crate) struct LiquidityLock {
    pub(crate) amount: NonZero<Collateral>,
    pub(crate) stats: LiquidityStats,
    pub(crate) price: PricePoint,
}

impl LiquidityLock {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        amount: NonZero<Collateral>,
        price: PricePoint,
        delta_notional: Option<Signed<Notional>>,
        net_notional_override: Option<Signed<Notional>>,
        // optional to allow chaining liquidity updates in validation, before applying
        stats: Option<LiquidityStats>,
    ) -> Result<Self> {
        let mut stats = stats.unwrap_or(state.load_liquidity_stats(store)?);
        let mut net_notional = match net_notional_override {
            Some(net_notional_override) => net_notional_override,
            None => {
                let long_interest_protocol = state.open_long_interest(store)?;
                let short_interest_protocol = state.open_short_interest(store)?;
                (long_interest_protocol.into_signed() - short_interest_protocol.into_signed())?
            }
        };
        net_notional =
            (net_notional + delta_notional.unwrap_or_else(|| Notional::zero().into_signed()))?;
        let min_unlocked_liquidity = state.min_unlocked_liquidity(net_notional, &price)?;

        if (min_unlocked_liquidity + amount.raw())? > stats.unlocked {
            Err(MarketError::Liquidity {
                requested: amount,
                total_unlocked: stats.unlocked,
                allowed: min_unlocked_liquidity,
            }
            .into_anyhow())
        } else {
            stats.locked = stats.locked.checked_add(amount.raw())?;
            stats.unlocked = stats.unlocked.checked_sub(amount.raw())?;
            Ok(Self {
                stats,
                amount,
                price,
            })
        }
    }

    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<()> {
        let Self {
            amount,
            stats,
            price,
        } = self;

        state.save_liquidity_stats(ctx.storage, &stats)?;

        // Technically the total pool size has not changed
        // but the event consists of both locked and unlocked parts
        // so emit the event for now
        state.add_pool_size_change_events(ctx, &stats, &price)?;

        ctx.response_mut().add_event(LockEvent { amount });

        Ok(())
    }
}

// Helper struct for liquidity "update lock"
// Update the amount of locked liquidity. Note, this does not update unlocked.
// TBD: can this be consolidated into LiquidityLock with a flag?
#[must_use]
pub(crate) struct LiquidityUpdateLocked {
    pub(crate) amount: Signed<Collateral>,
    pub(crate) price: PricePoint,
    pub(crate) stats: LiquidityStats,
}

impl LiquidityUpdateLocked {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        amount: Signed<Collateral>,
        price: PricePoint,
        // optional to allow chaining liquidity updates in validation, before applying
        stats: Option<LiquidityStats>,
    ) -> Result<Self> {
        let mut stats = stats.unwrap_or(state.load_liquidity_stats(store)?);
        stats.locked = match stats
            .locked
            .into_signed()
            .checked_add(amount)?
            .try_into_non_negative_value()
        {
            None => anyhow::bail!(
                "liquidity_update_locked: locked is {}, amount is {}",
                stats.locked,
                amount
            ),
            Some(locked) => locked,
        };

        Ok(Self {
            amount,
            price,
            stats,
        })
    }

    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<()> {
        let Self {
            amount,
            price,
            stats,
        } = self;

        state.save_liquidity_stats(ctx.storage, &stats)?;

        // The total pool size *has* changed here, due to LPs winning or losing
        // at the time of liquifunding
        state.add_pool_size_change_events(ctx, &stats, &price)?;

        ctx.response_mut().add_event(LockUpdateEvent { amount });

        Ok(())
    }
}

#[must_use]
pub(crate) struct LiquidityNewYieldToProcess {
    pub(crate) next_index: u64,
    pub(crate) new_yield: YieldPerToken,
}

impl LiquidityNewYieldToProcess {
    pub(crate) fn apply(self, _state: &State, ctx: &mut StateContext) -> Result<()> {
        YIELD_PER_TIME_PER_TOKEN.save(ctx.storage, self.next_index, &self.new_yield)?;
        Ok(())
    }
}
