mod aggregate_capping;
pub(crate) mod borrow_fees;

use crate::prelude::*;
use crate::state::data_series::DataSeries;
use anyhow::Context;
use cosmwasm_std::Decimal256;
use msg::contracts::market::fees::events::{
    BorrowFeeChangeEvent, FeeType, FundingPaymentEvent, FundingRateChangeEvent,
    InsufficientMarginEvent, TradeId,
};
use msg::contracts::market::position::{
    ClosePositionInstructions, LiquidationReason, MaybeClosedPosition, Position,
    PositionCloseReason,
};

use self::aggregate_capping::aggregate_capping;
use self::borrow_fees::BorrowFees;

pub(super) const LP_BORROW_FEE_DATA_SERIES: DataSeries =
    DataSeries::new(namespace::LP_BORROW_FEE_DATA_SERIES);

pub(super) const XLP_BORROW_FEE_DATA_SERIES: DataSeries =
    DataSeries::new(namespace::XLP_BORROW_FEE_DATA_SERIES);

const LONG_RF_PRICE_PREFIX_SUM: DataSeries = DataSeries::new(namespace::FUNDING_RATE_LONG);

const SHORT_RF_PRICE_PREFIX_SUM: DataSeries = DataSeries::new(namespace::FUNDING_RATE_SHORT);

/// The total net funding payments across the lifetime of the market.
///
/// The sign here matches the sign on individual funding payments: a positive
/// value means, in total, more money has flowed into the contract than has
/// flowed out. A negative value means that, in aggregate, more money has flowed
/// out. We need to enforce the invariant that `total_margin + total_paid >= 0`.
const TOTAL_NET_FUNDING_PAID: Item<Signed<Collateral>> =
    Item::new(namespace::TOTAL_NET_FUNDING_PAID);

/// The sum of the funding payment portion of liquidation margin across all open
/// positions.
const TOTAL_FUNDING_MARGIN: Item<Collateral> = Item::new(namespace::TOTAL_FUNDING_MARGIN);

/// Assert that the invariants of funding payments are met.
///
/// This function takes an extra position margin parameter to represent an
/// amount to be taken away from the total margin.
#[cfg(debug_assertions)]
fn debug_check_invariants(store: &dyn Storage, pos_margin: Collateral) {
    let total_paid = TOTAL_NET_FUNDING_PAID.load(store).unwrap();
    let total_margin =
        TOTAL_FUNDING_MARGIN.load(store).unwrap().into_signed() - pos_margin.into_signed();
    let sum = total_paid + total_margin;

    // Add an epsilon to account for rounding errors
    let sum = sum + Signed::<Collateral>::from_number(Number::EPS_E7);
    assert!(
        sum.is_positive_or_zero(),
        "Total funding payments invariants failed. Margin: {total_margin}. Paid: {total_paid}"
    );
}

impl State<'_> {
    fn calculate_funding_payment(
        &self,
        store: &dyn Storage,
        pos: &Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
    ) -> Result<Signed<Collateral>> {
        const NS_PER_YEAR: u128 = 31_536_000_000_000_000;

        Ok(Signed::<Collateral>::from_number(
            match pos.direction() {
                DirectionToNotional::Long => LONG_RF_PRICE_PREFIX_SUM
                    .sum(store, starts_at, ends_at)
                    .unwrap_or(Number::ZERO),
                DirectionToNotional::Short => SHORT_RF_PRICE_PREFIX_SUM
                    .sum(store, starts_at, ends_at)
                    .unwrap_or(Number::ZERO),
            } * pos.notional_size.abs().into_number()
                / Number::from(NS_PER_YEAR),
        ))
    }

    fn calculate_borrow_fee_payment(
        &self,
        store: &dyn Storage,
        pos: &Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
    ) -> Result<LpAndXlp> {
        const NS_PER_YEAR: u128 = 31_536_000_000_000_000;

        let lp_instant_rate = LP_BORROW_FEE_DATA_SERIES.sum(store, starts_at, ends_at)?;
        let xlp_instant_rate = XLP_BORROW_FEE_DATA_SERIES.sum(store, starts_at, ends_at)?;

        let lp = Collateral::try_from_number(
            lp_instant_rate * pos.counter_collateral.into_number() / Number::from(NS_PER_YEAR),
        )?;
        let xlp = Collateral::try_from_number(
            xlp_instant_rate * pos.counter_collateral.into_number() / Number::from(NS_PER_YEAR),
        )?;

        Ok(LpAndXlp { lp, xlp })
    }

    pub(crate) fn get_current_borrow_fee_rate_annual(
        &self,
        store: &dyn Storage,
    ) -> Result<(Timestamp, BorrowFees)> {
        let (lp_timestamp, lp_rate) = LP_BORROW_FEE_DATA_SERIES
            .try_load_last(store)?
            .context("get_current_borrow_fee_rate_annual: No initial lp borrow fee set")?;

        let (xlp_timestamp, xlp_rate) = XLP_BORROW_FEE_DATA_SERIES
            .try_load_last(store)?
            .context("get_current_borrow_fee_rate_annual: No initial xlp borrow fee set")?;

        debug_assert_eq!(lp_timestamp, xlp_timestamp);

        Ok((
            lp_timestamp,
            BorrowFees {
                lp: lp_rate
                    .value
                    .try_into_positive_value()
                    .context("lp_rate is negative")?,
                xlp: xlp_rate
                    .value
                    .try_into_positive_value()
                    .context("xlp_rate is negative")?,
            },
        ))
    }

    pub(crate) fn derive_instant_borrow_fee_rate_annual(
        &self,
        store: &dyn Storage,
    ) -> Result<BorrowFees> {
        // See section 5.5 of the whitepaper

        let (previous_rate_time, previous_rate) = self.get_current_borrow_fee_rate_annual(store)?;
        let now = self.now();
        let nanos_since_last_rate = if previous_rate_time < now {
            (now.checked_sub(previous_rate_time, "derive_instant_borrow_fee_rate_annual")?)
                .as_nanos()
        } else {
            0
        };
        const NS_PER_SECOND: u128 = 1_000_000_000;
        const NS_PER_DAY: u128 = NS_PER_SECOND * 60 * 60 * 24;
        let stats = self.load_liquidity_stats(store)?;

        let bias = if stats.locked.is_zero() && stats.unlocked.is_zero() {
            // No liquidity in the system, we need to push the borrow rate up
            Number::NEG_ONE
        } else {
            let actual_utilization = stats
                .locked
                .into_decimal256()
                .checked_div(stats.locked.checked_add(stats.unlocked)?.into_decimal256())?;
            actual_utilization
                .into_number()
                .checked_sub(self.config.target_utilization.into_number())?
        };

        let rate_delta = Number::from(self.config.borrow_fee_sensitivity)
            .checked_mul(bias)?
            .checked_mul(Number::from(nanos_since_last_rate))?
            .checked_div(Number::from(NS_PER_DAY))?;
        let calculated_rate = previous_rate
            .total()
            .into_number()
            .checked_add(rate_delta)?;
        let total_rate = calculated_rate
            .try_into_positive_value()
            .and_then(NonZero::new)
            .map_or(
                self.config.borrow_fee_rate_min_annualized,
                |calculated_rate| {
                    calculated_rate.clamp(
                        self.config.borrow_fee_rate_min_annualized,
                        self.config.borrow_fee_rate_max_annualized,
                    )
                },
            )
            .raw();

        // Calculate the portion that goes to xLP, we'll get the LP as the difference
        Ok(if stats.total_xlp.is_zero() {
            // No xLP in the system, give everything to LP
            //
            // If there's no LP either, this number doesn't matter anyway,
            // because no one is able to borrow anything anyway.
            BorrowFees {
                lp: total_rate,
                xlp: Decimal256::zero(),
            }
        } else if stats.total_lp.is_zero() {
            // No LP in the system, so give everything to xLP
            BorrowFees {
                lp: Decimal256::zero(),
                xlp: total_rate,
            }
        } else {
            // Linear interpolation to calculate the multiplier for xLP versus LP
            let multiplier = calc_multiplier(
                self.config.min_xlp_rewards_multiplier.raw(),
                self.config.max_xlp_rewards_multiplier.raw(),
                stats.total_lp,
                stats.total_xlp,
            );

            let lp_shares = stats.total_lp;
            let xlp_shares =
                LpToken::from_decimal256(stats.total_xlp.into_decimal256() * multiplier);
            let shares = lp_shares + xlp_shares;

            let lp = total_rate * lp_shares.into_decimal256() / shares.into_decimal256();
            let xlp = total_rate - lp; // use subtraction to avoid rounding errors
            BorrowFees { lp, xlp }
        })
    }

    /**
     * Derive the instantaneous (long, short) funding rate
     */
    pub(crate) fn derive_instant_funding_rate_annual(
        &self,
        store: &dyn Storage,
    ) -> Result<(Number, Number)> {
        let config = &self.config;
        let rf_per_annual_cap = config.funding_rate_max_annualized;

        let instant_net_open_interest = self.positions_net_open_interest(store)?;
        let instant_open_short = self.open_short_interest(store)?;
        let instant_open_long = self.open_long_interest(store)?;
        let funding_rate_sensitivity = config.funding_rate_sensitivity;

        let total_interest = (instant_open_long + instant_open_short).into_decimal256();
        let notional_high_cap = config.delta_neutrality_fee_sensitivity.into_decimal256()
            * config.delta_neutrality_fee_cap.into_decimal256();
        let funding_rate_sensitivity_from_delta_neutrality =
            rf_per_annual_cap * total_interest / notional_high_cap;

        let effective_funding_rate_sensitivity =
            funding_rate_sensitivity.max(funding_rate_sensitivity_from_delta_neutrality);
        let rf_popular = || -> Result<Decimal256> {
            Ok(std::cmp::min(
                effective_funding_rate_sensitivity
                    * (instant_net_open_interest.abs_unsigned().into_decimal256()
                        / (instant_open_long + instant_open_short).into_decimal256()),
                rf_per_annual_cap,
            ))
        };

        let rf_unpopular = || -> Result<Decimal256> {
            match instant_open_long.cmp(&instant_open_short) {
                std::cmp::Ordering::Greater => Ok(rf_popular()?
                    * (instant_open_long.into_decimal256() / instant_open_short.into_decimal256())),
                std::cmp::Ordering::Less => Ok(rf_popular()?
                    * (instant_open_short.into_decimal256() / instant_open_long.into_decimal256())),
                std::cmp::Ordering::Equal => Ok(Decimal256::zero()),
            }
        };

        let (long_rate, short_rate) = if instant_open_long.is_zero() || instant_open_short.is_zero()
        {
            // When all on one side, popular side has no one to pay
            (Number::ZERO, Number::ZERO)
        } else {
            match instant_open_long.cmp(&instant_open_short) {
                std::cmp::Ordering::Greater => {
                    (rf_popular()?.into_signed(), -rf_unpopular()?.into_signed())
                }
                std::cmp::Ordering::Less => {
                    (-rf_unpopular()?.into_signed(), rf_popular()?.into_signed())
                }
                std::cmp::Ordering::Equal => (Number::ZERO, Number::ZERO),
            }
        };

        Ok((long_rate, short_rate))
    }

    pub(crate) fn accumulate_funding_rate(
        &self,
        ctx: &mut StateContext,
        time: Timestamp,
    ) -> Result<()> {
        let spot_price = self.spot_price(ctx.storage, Some(time))?.price_notional;

        let (long_rate, short_rate) = self.derive_instant_funding_rate_annual(ctx.storage)?;
        LONG_RF_PRICE_PREFIX_SUM.append(ctx.storage, time, long_rate * spot_price.into_number())?;

        SHORT_RF_PRICE_PREFIX_SUM.append(
            ctx.storage,
            time,
            short_rate * spot_price.into_number(),
        )?;

        let (long_rate_base, short_rate_base) = match self.market_id(ctx.storage)?.get_market_type()
        {
            MarketType::CollateralIsQuote => (long_rate, short_rate),
            MarketType::CollateralIsBase => (short_rate, long_rate),
        };

        ctx.response_mut().add_event(FundingRateChangeEvent {
            time,
            long_rate_base,
            short_rate_base,
        });

        Ok(())
    }

    /// Initialize the borrow fee data structure to contain the min fee
    pub(crate) fn initialize_borrow_fee_rate(
        &self,
        ctx: &mut StateContext,
        initial_rate: Decimal256,
    ) -> Result<()> {
        // Check validity of the rate
        anyhow::ensure!(
            initial_rate >= self.config.borrow_fee_rate_min_annualized.raw(),
            "Initial borrow rate must be at least minimum rate"
        );
        anyhow::ensure!(
            initial_rate <= self.config.borrow_fee_rate_max_annualized.raw(),
            "Initial borrow rate must be at most maximum rate"
        );

        LP_BORROW_FEE_DATA_SERIES.append(ctx.storage, self.now(), initial_rate.into())?;
        XLP_BORROW_FEE_DATA_SERIES.append(ctx.storage, self.now(), Number::ZERO)
    }

    pub(crate) fn accumulate_borrow_fee_rate(
        &self,
        ctx: &mut StateContext,
        time: Timestamp,
    ) -> Result<()> {
        let rate = self
            .derive_instant_borrow_fee_rate_annual(ctx.storage)
            .context("derive_instant_borrow_fee_rate_annual")?;

        LP_BORROW_FEE_DATA_SERIES.append(ctx.storage, time, rate.lp.into_number())?;
        XLP_BORROW_FEE_DATA_SERIES.append(ctx.storage, time, rate.xlp.into_number())?;

        ctx.response.add_event(BorrowFeeChangeEvent {
            time,
            total_rate: rate.lp.checked_add(rate.xlp)?,
            lp_rate: rate.lp,
            xlp_rate: rate.xlp,
        });

        Ok(())
    }

    pub(crate) fn funding_valid_until(&self, store: &dyn Storage) -> Result<Timestamp> {
        Ok(self
            .next_crank_timestamp(store)?
            .map_or(self.now(), |price_point| price_point.timestamp))
    }

    /// Calculate the capped borrow fee amount to be paid.
    ///
    /// If capping occurs, includes the event to be returned in the response.
    pub(crate) fn calc_capped_borrow_fee_payment(
        &self,
        store: &dyn Storage,
        pos: &Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
    ) -> Result<(LpAndXlp, Option<InsufficientMarginEvent>)> {
        let uncapped = self.calculate_borrow_fee_payment(store, pos, starts_at, ends_at)?;

        let total = uncapped.lp.checked_add(uncapped.xlp)?;
        let total = match NonZero::new(total) {
            Some(total) => total,
            // Zero to pay, nothing to worry about
            None => return Ok((uncapped, None)),
        };
        let cap = pos.liquidation_margin.borrow;
        Ok(if total.raw() <= cap {
            (uncapped, None)
        } else {
            let event = InsufficientMarginEvent {
                pos: pos.id,
                fee_type: FeeType::Borrow,
                available: cap.into_signed(),
                requested: total.into_signed(),
                desc: None,
            };

            // Scale down the LP amount
            let scale = cap.div_non_zero(total);
            let lp = uncapped.lp.checked_mul_dec(scale)?;

            // Avoid rounding errors: make the xLP amount the difference
            let xlp = cap.checked_sub(lp)?;

            (LpAndXlp { lp, xlp }, Some(event))
        })
    }

    /// Calculate the capped funding payment amount to be paid.
    ///
    /// If capping occurs, includes the event to be returned in the response.
    pub(crate) fn calc_capped_funding_payment(
        &self,
        store: &dyn Storage,
        pos: &Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
    ) -> Result<(Signed<Collateral>, Option<InsufficientMarginEvent>)> {
        let funding_ends_at = self.funding_valid_until(store)?.min(ends_at);
        let uncapped_signed =
            self.calculate_funding_payment(store, pos, starts_at, funding_ends_at)?;

        // If negative or zero, position doesn't have to pay, so we can't have hit our cap.
        let uncapped = match NonZero::try_from_signed(uncapped_signed).ok() {
            None => return Ok((uncapped_signed, None)),
            Some(uncapped) => uncapped,
        };
        let cap = pos.liquidation_margin.funding;
        Ok(if uncapped.raw() <= cap {
            (uncapped_signed, None)
        } else {
            let event = InsufficientMarginEvent {
                pos: pos.id,
                fee_type: FeeType::Funding,
                available: cap.into_signed(),
                requested: uncapped.into_signed(),
                desc: None,
            };

            (cap.into_signed(), Some(event))
        })
    }

    /// Cap the crank fee, emitting an event if the capping was triggered.
    fn cap_crank_fee(
        &self,
        ctx: &mut StateContext,
        pos: &Position,
        price_point: &PricePoint,
        uncapped_usd: Usd,
    ) -> Result<(Collateral, Usd)> {
        let uncapped = price_point.usd_to_collateral(uncapped_usd);
        let uncapped = match NonZero::new(uncapped) {
            Some(uncapped) => uncapped,
            None => {
                debug_assert_eq!(uncapped_usd, Usd::zero());
                return Ok((uncapped, uncapped_usd));
            }
        };

        let cap = pos.liquidation_margin.crank;
        Ok(if uncapped.raw() <= cap {
            (uncapped.raw(), uncapped_usd)
        } else {
            ctx.response_mut().add_event(InsufficientMarginEvent {
                pos: pos.id,
                fee_type: FeeType::Crank,
                available: cap.into_signed(),
                requested: uncapped.into_signed(),
                desc: None,
            });

            let cap_usd = price_point.collateral_to_usd(cap);
            (cap, cap_usd)
        })
    }

    /// Settle pending fees
    pub(crate) fn position_settle_pending_fees(
        &self,
        ctx: &mut StateContext,
        mut position: Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
        charge_crank_fee: bool,
    ) -> Result<MaybeClosedPosition> {
        let price = self.spot_price(ctx.storage, None)?;

        let (borrow_fee_timeslice_owed, event) =
            self.calc_capped_borrow_fee_payment(ctx.storage, &position, starts_at, ends_at)?;
        if let Some(event) = event {
            ctx.response_mut().add_event(event);
        }

        let (funding_timeslice_owed, event) =
            self.calc_capped_funding_payment(ctx.storage, &position, starts_at, ends_at)?;
        if let Some(event) = event {
            ctx.response_mut().add_event(event);
        }
        let funding_timeslice_owed =
            self.track_funding_fee_payment_with_capping(ctx, funding_timeslice_owed, &position)?;

        position
            .funding_fee
            .checked_add_assign(funding_timeslice_owed, &price)?;

        position.borrow_fee.checked_add_assign(
            borrow_fee_timeslice_owed.lp + borrow_fee_timeslice_owed.xlp,
            &price,
        )?;

        // collect the borrow fee portion, which is ultimately paid out to liquidity providers
        // funding portion is paid between positions and dealt with through
        // the inherent position bookkeeping, not a global fee collection
        self.collect_borrow_fee(ctx, position.id, borrow_fee_timeslice_owed, price)?;

        let market_type = self.market_id(ctx.storage)?.get_market_type();

        ctx.response_mut().add_event(FundingPaymentEvent {
            pos_id: position.id,
            amount: funding_timeslice_owed,
            amount_usd: funding_timeslice_owed.map(|x| price.collateral_to_usd(x)),
            direction: position.direction().into_base(market_type),
        });

        let total_crank_fee_usd = if charge_crank_fee {
            position
                .pending_crank_fee
                .checked_add(self.config.crank_fee_charged)?
        } else {
            position.pending_crank_fee
        };
        position.pending_crank_fee = Usd::zero();

        let crank_fee_charged = if total_crank_fee_usd.is_zero() {
            Collateral::zero()
        } else {
            let (crank_fee, crank_fee_usd) =
                self.cap_crank_fee(ctx, &position, &price, total_crank_fee_usd)?;
            self.collect_crank_fee(
                ctx,
                TradeId::Position(position.id),
                crank_fee,
                crank_fee_usd,
            )?;
            position.crank_fee.checked_add_assign(crank_fee, &price)?;
            crank_fee
        };

        // Update the active collateral
        debug_assert!(position.active_collateral.raw() >= position.liquidation_margin.total());
        let to_subtract = borrow_fee_timeslice_owed.lp.into_signed()
            + borrow_fee_timeslice_owed.xlp.into_signed()
            + funding_timeslice_owed
            + crank_fee_charged.into_signed();
        debug_assert!(to_subtract <= position.liquidation_margin.total().into_signed());

        Ok(
            match position
                .active_collateral
                .checked_sub_signed(to_subtract)
                .ok()
            {
                Some(active_collateral) => {
                    position.active_collateral = active_collateral;
                    MaybeClosedPosition::Open(position)
                }
                // This is a deeply degenerate case that should never happen,
                // but we check for it explicitly and handle as gracefully as
                // possible.
                None => {
                    if let Some(to_subtract) = to_subtract.try_into_non_zero() {
                        ctx.response_mut().add_event(InsufficientMarginEvent {
                            pos: position.id,
                            fee_type: FeeType::Overall,
                            available: position.active_collateral.into_signed(),
                            requested: to_subtract.into_signed(),
                            desc: None,
                        });
                    } else {
                        debug_assert!(
                            false,
                            "Impossible! to_subtract is not a strictly positive value"
                        );
                    }
                    // Nothing better to do here, just provide a tiny amount of active collateral.
                    position.active_collateral = "0.0001".parse().unwrap();
                    MaybeClosedPosition::Close(ClosePositionInstructions {
                        pos: position,
                        exposure: Signed::<Collateral>::zero(),
                        close_time: ends_at,
                        settlement_time: ends_at,
                        reason: PositionCloseReason::Liquidated(LiquidationReason::Liquidated),
                    })
                }
            },
        )
    }

    /// Initialize the funding totals data structures
    pub(crate) fn initialize_funding_totals(&self, ctx: &mut StateContext) -> Result<()> {
        TOTAL_NET_FUNDING_PAID.save(ctx.storage, &Signed::<Collateral>::zero())?;
        TOTAL_FUNDING_MARGIN.save(ctx.storage, &Collateral::zero())?;
        Ok(())
    }

    /// Update the [TOTAL_NET_FUNDING_PAID] [Item] with the given payment.
    ///
    /// This function also caps the amount sent in to ensure we don't pay out
    /// more than the funds available in the margin. If such a capping occurs,
    /// we emit an event.
    ///
    /// Negative input == outgoing payment
    fn track_funding_fee_payment_with_capping(
        &self,
        ctx: &mut StateContext,
        amount: Signed<Collateral>,
        pos: &Position,
    ) -> Result<Signed<Collateral>> {
        let total_paid = TOTAL_NET_FUNDING_PAID.load(ctx.storage)?;
        let total_margin = TOTAL_FUNDING_MARGIN.load(ctx.storage)?;

        let capping = aggregate_capping(
            total_paid,
            total_margin,
            amount,
            pos.liquidation_margin.funding,
        )?;

        let amount = match capping {
            aggregate_capping::AggregateCapping::NoCapping => amount,
            aggregate_capping::AggregateCapping::Capped { capped_amount } => {
                // For the event, we negate both values, since the event
                // considers positive values flows out of the system.
                ctx.response_mut().add_event(InsufficientMarginEvent {
                    pos: pos.id,
                    fee_type: FeeType::FundingTotal,
                    available: -capped_amount,
                    requested: -amount,
                    desc: Some(format!("Protocol-level insufficient funding total. Total paid: {total_paid}. Total margin: {total_margin}. Amount requested: {amount}. Funding margin: {}", pos.liquidation_margin.funding))
                });
                capped_amount
            }
        };

        TOTAL_NET_FUNDING_PAID.save(ctx.storage, &total_paid.checked_add(amount)?)?;

        #[cfg(debug_assertions)]
        debug_check_invariants(ctx.storage, pos.liquidation_margin.funding);

        Ok(amount)
    }

    /// Add a margin value to [TOTAL_FUNDING_MARGIN]
    pub(crate) fn increase_total_funding_margin(
        &self,
        ctx: &mut StateContext,
        amount: Collateral,
    ) -> Result<()> {
        TOTAL_FUNDING_MARGIN.update(ctx.storage, |x| {
            x.checked_add(amount).context("Too much funding margin")
        })?;

        #[cfg(debug_assertions)]
        debug_check_invariants(ctx.storage, Collateral::zero());

        Ok(())
    }

    /// Subtract a margin value from [TOTAL_FUNDING_MARGIN]
    pub(crate) fn decrease_total_funding_margin(
        &self,
        ctx: &mut StateContext,
        amount: Collateral,
    ) -> Result<()> {
        TOTAL_FUNDING_MARGIN.update(ctx.storage, |x| {
            x.checked_sub(amount).context("Too much funding margin")
        })?;

        #[cfg(debug_assertions)]
        debug_check_invariants(ctx.storage, Collateral::zero());

        Ok(())
    }
}

#[cfg(feature = "sanity")]
/// Get the current [TOTAL_NET_FUNDING_PAID]
pub(crate) fn get_total_net_funding_paid(store: &dyn Storage) -> Result<Signed<Collateral>> {
    TOTAL_NET_FUNDING_PAID.load(store).map_err(|e| e.into())
}

#[cfg(feature = "sanity")]
/// Get the current [TOTAL_FUNDING_MARGIN]
pub(crate) fn get_total_funding_margin(store: &dyn Storage) -> Result<Collateral> {
    TOTAL_FUNDING_MARGIN.load(store).map_err(|e| e.into())
}

/// Helper type to combine LP and xLP values together, whatever they happen to be.
///
/// Main purpose of this is to avoid anonymous pairs where it's easy to mix up
/// the order of values.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LpAndXlp {
    pub(crate) lp: Collateral,
    pub(crate) xlp: Collateral,
}

impl LpAndXlp {
    pub(crate) fn zero() -> Self {
        LpAndXlp {
            lp: Collateral::zero(),
            xlp: Collateral::zero(),
        }
    }
}

/// Calculate the multiplier of xLP versus LP rewards
fn calc_multiplier(
    min_multiplier: Decimal256,
    max_multiplier: Decimal256,
    total_lp: LpToken,
    total_xlp: LpToken,
) -> Decimal256 {
    debug_assert!(!(total_lp.is_zero() && total_xlp.is_zero()));
    (min_multiplier * total_xlp.into_decimal256() + max_multiplier * total_lp.into_decimal256())
        / (total_lp + total_xlp).into_decimal256()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn go(
        min_multiplier: &str,
        max_multiplier: &str,
        total_lp: &str,
        total_xlp: &str,
    ) -> Decimal256 {
        calc_multiplier(
            min_multiplier.parse().unwrap(),
            max_multiplier.parse().unwrap(),
            total_lp.parse().unwrap(),
            total_xlp.parse().unwrap(),
        )
    }

    #[test]
    fn all_lp() {
        assert_eq!(go("2", "5", "500", "0"), Decimal256::from_str("5").unwrap());
    }

    #[test]
    fn all_xlp() {
        assert_eq!(go("2", "5", "0", "500"), Decimal256::from_str("2").unwrap());
    }

    #[test]
    fn even_split() {
        assert_eq!(
            go("2", "5", "500", "500"),
            Decimal256::from_str("3.5").unwrap()
        );
    }
}
