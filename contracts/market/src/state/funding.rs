mod aggregate_capping;
pub(crate) mod borrow_fees;

use crate::prelude::*;
use crate::state::data_series::DataSeries;
use anyhow::Context;
use cosmwasm_std::{Decimal256, OverflowError};
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

use super::fees::{BorrowFeeCollection, CapCrankFee};

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
    let total_margin = (TOTAL_FUNDING_MARGIN.load(store).unwrap().into_signed()
        - pos_margin.into_signed())
    .unwrap();
    let sum = (total_paid + total_margin).unwrap();

    // Add an epsilon to account for rounding errors
    let sum = sum + Signed::<Collateral>::from_number(Number::EPS_E7);
    assert!(
        sum.unwrap().is_positive_or_zero(),
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
            ((match pos.direction() {
                DirectionToNotional::Long => LONG_RF_PRICE_PREFIX_SUM
                    .sum(store, starts_at, ends_at)
                    .unwrap_or(Number::ZERO),
                DirectionToNotional::Short => SHORT_RF_PRICE_PREFIX_SUM
                    .sum(store, starts_at, ends_at)
                    .unwrap_or(Number::ZERO),
            } * pos.notional_size.abs().into_number())?
                / Number::from(NS_PER_YEAR))?,
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
            ((lp_instant_rate * pos.counter_collateral.into_number())?
                / Number::from(NS_PER_YEAR))?,
        )?;
        let xlp = Collateral::try_from_number(
            ((xlp_instant_rate * pos.counter_collateral.into_number())?
                / Number::from(NS_PER_YEAR))?,
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
                    .try_into_non_negative_value()
                    .context("lp_rate is negative")?,
                xlp: xlp_rate
                    .value
                    .try_into_non_negative_value()
                    .context("xlp_rate is negative")?,
            },
        ))
    }

    pub(crate) fn derive_instant_borrow_fee_rate_annual(
        &self,
        store: &dyn Storage,
        price_point: &PricePoint,
    ) -> Result<BorrowFees> {
        // See section 5.5 of the whitepaper

        let (previous_rate_time, previous_rate) = self.get_current_borrow_fee_rate_annual(store)?;
        let nanos_since_last_rate = if previous_rate_time < price_point.timestamp {
            (price_point
                .timestamp
                .checked_sub(previous_rate_time, "derive_instant_borrow_fee_rate_annual")?)
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
            .total()?
            .into_number()
            .checked_add(rate_delta)?;
        let total_rate = calculated_rate
            .try_into_non_negative_value()
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
            )?;

            let lp_shares = stats.total_lp;
            let xlp_shares = LpToken::from_decimal256(
                stats.total_xlp.into_decimal256().checked_mul(multiplier)?,
            );
            let shares = (lp_shares + xlp_shares)?;

            let lp = total_rate
                .checked_mul(lp_shares.into_decimal256())?
                .checked_div(shares.into_decimal256())?;

            let xlp = total_rate.checked_sub(lp)?; // use subtraction to avoid rounding errors

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

        let total_interest = (instant_open_long + instant_open_short)?.into_decimal256();
        let notional_high_cap = config
            .delta_neutrality_fee_sensitivity
            .into_decimal256()
            .checked_mul(config.delta_neutrality_fee_cap.into_decimal256())?;
        let funding_rate_sensitivity_from_delta_neutrality = rf_per_annual_cap
            .checked_mul(total_interest)?
            .checked_div(notional_high_cap)?;

        let effective_funding_rate_sensitivity =
            funding_rate_sensitivity.max(funding_rate_sensitivity_from_delta_neutrality);
        let rf_popular = || -> Result<Decimal256> {
            Ok(std::cmp::min(
                effective_funding_rate_sensitivity.checked_mul(
                    instant_net_open_interest
                        .abs_unsigned()
                        .into_decimal256()
                        .checked_div((instant_open_long + instant_open_short)?.into_decimal256())?,
                )?,
                rf_per_annual_cap,
            ))
        };

        let rf_unpopular = || -> Result<Decimal256> {
            match instant_open_long.cmp(&instant_open_short) {
                std::cmp::Ordering::Greater => Ok(rf_popular()?.checked_mul(
                    instant_open_long
                        .into_decimal256()
                        .checked_div(instant_open_short.into_decimal256())?,
                )?),
                std::cmp::Ordering::Less => Ok(rf_popular()?.checked_mul(
                    instant_open_short
                        .into_decimal256()
                        .checked_div(instant_open_long.into_decimal256())?,
                )?),
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
        price_point: &PricePoint,
    ) -> Result<()> {
        let time = price_point.timestamp;
        let spot_price = price_point.price_notional;

        let (long_rate, short_rate) = self.derive_instant_funding_rate_annual(ctx.storage)?;
        LONG_RF_PRICE_PREFIX_SUM.append(
            ctx.storage,
            time,
            (long_rate * spot_price.into_number())?,
        )?;

        SHORT_RF_PRICE_PREFIX_SUM.append(
            ctx.storage,
            time,
            (short_rate * spot_price.into_number())?,
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

        // Usage of self.now() is acceptable here, it's only used for
        // initializing data structures during contract instantiation.
        LP_BORROW_FEE_DATA_SERIES.append(ctx.storage, self.now(), initial_rate.into())?;
        XLP_BORROW_FEE_DATA_SERIES.append(ctx.storage, self.now(), Number::ZERO)
    }

    pub(crate) fn accumulate_borrow_fee_rate(
        &self,
        ctx: &mut StateContext,
        price_point: &PricePoint,
    ) -> Result<()> {
        let rate = self
            .derive_instant_borrow_fee_rate_annual(ctx.storage, price_point)
            .context("derive_instant_borrow_fee_rate_annual")?;

        LP_BORROW_FEE_DATA_SERIES.append(
            ctx.storage,
            price_point.timestamp,
            rate.lp.into_number(),
        )?;
        XLP_BORROW_FEE_DATA_SERIES.append(
            ctx.storage,
            price_point.timestamp,
            rate.xlp.into_number(),
        )?;

        ctx.response.add_event(BorrowFeeChangeEvent {
            time: price_point.timestamp,
            total_rate: rate.lp.checked_add(rate.xlp)?,
            lp_rate: rate.lp,
            xlp_rate: rate.xlp,
        });

        Ok(())
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
        is_query: bool,
    ) -> Result<(Signed<Collateral>, Option<InsufficientMarginEvent>)> {
        debug_assert!(
            ends_at <= pos.next_liquifunding || is_query,
            "calc_capped_funding_payment: ends_at {ends_at} greater than next_liquifunding {}",
            pos.next_liquifunding
        );
        let uncapped_signed = self.calculate_funding_payment(store, pos, starts_at, ends_at)?;

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
        pos: &Position,
        price_point: &PricePoint,
        uncapped_usd: Usd,
    ) -> Result<CapCrankFee> {
        let trade_id = TradeId::Position(pos.id);
        let uncapped = price_point.usd_to_collateral(uncapped_usd);
        let uncapped = match NonZero::new(uncapped) {
            Some(uncapped) => uncapped,
            None => {
                debug_assert_eq!(uncapped_usd, Usd::zero());
                return Ok(CapCrankFee {
                    trade_id,
                    amount: uncapped,
                    amount_usd: uncapped_usd,
                    insufficient_margin_event: None,
                });
            }
        };

        let cap = pos.liquidation_margin.crank;
        Ok(if uncapped.raw() <= cap {
            CapCrankFee {
                trade_id,
                amount: uncapped.raw(),
                amount_usd: uncapped_usd,
                insufficient_margin_event: None,
            }
        } else {
            CapCrankFee {
                trade_id,
                insufficient_margin_event: Some(InsufficientMarginEvent {
                    pos: pos.id,
                    fee_type: FeeType::Crank,
                    available: cap.into_signed(),
                    requested: uncapped.into_signed(),
                    desc: None,
                }),
                amount: cap,
                amount_usd: price_point.collateral_to_usd(cap),
            }
        })
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
        store: &dyn Storage,
        amount: Signed<Collateral>,
        pos: &Position,
    ) -> Result<FundingFeePaymentWithCapping> {
        let total_paid = TOTAL_NET_FUNDING_PAID.load(store)?;
        let total_margin = TOTAL_FUNDING_MARGIN.load(store)?;

        let capping = aggregate_capping(
            total_paid,
            total_margin,
            amount,
            pos.liquidation_margin.funding,
        )?;

        let (amount, insufficient_margin_event) = match capping {
            aggregate_capping::AggregateCapping::NoCapping => (amount, None),
            aggregate_capping::AggregateCapping::Capped { capped_amount } => {
                // For the event, we negate both values, since the event
                // considers positive values flows out of the system.
                (capped_amount, Some(InsufficientMarginEvent {
                    pos: pos.id,
                    fee_type: FeeType::FundingTotal,
                    available: -capped_amount,
                    requested: -amount,
                    desc: Some(format!("Protocol-level insufficient funding total. Total paid: {total_paid}. Total margin: {total_margin}. Amount requested: {amount}. Funding margin: {}", pos.liquidation_margin.funding))
                }))
            }
        };

        let total_net_funding_paid = total_paid.checked_add(amount)?;

        Ok(FundingFeePaymentWithCapping {
            amount,
            insufficient_margin_event,
            total_net_funding_paid,
            #[cfg(debug_assertions)]
            margin_to_check: pos.liquidation_margin.funding,
        })
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
) -> Result<Decimal256, OverflowError> {
    debug_assert!(!(total_lp.is_zero() && total_xlp.is_zero()));
    Ok((min_multiplier
        .checked_mul(total_xlp.into_decimal256())?
        .checked_add(max_multiplier.checked_mul(total_lp.into_decimal256())?))?
        / (total_lp + total_xlp)?.into_decimal256())
}

#[must_use]
pub(crate) struct PositionFeeSettlement {
    pub(crate) position: MaybeClosedPosition,
    pub(crate) insufficient_margin_events: Vec<InsufficientMarginEvent>,
    pub(crate) funding_fee_payment: FundingFeePaymentWithCapping,
    pub(crate) borrow_fee_collection: BorrowFeeCollection,
    pub(crate) funding_payment_event: FundingPaymentEvent,
    pub(crate) cap_crank_fee: Option<CapCrankFee>,
}

impl PositionFeeSettlement {
    /// Settle pending fees
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        mut position: Position,
        starts_at: Timestamp,
        ends_at: Timestamp,
        charge_crank_fee: bool,
    ) -> Result<Self> {
        let price = state.spot_price(store, ends_at)?;

        let mut insufficient_margin_events = Vec::new();
        let (borrow_fee_timeslice_owed, event) =
            state.calc_capped_borrow_fee_payment(store, &position, starts_at, ends_at)?;

        if let Some(event) = event {
            insufficient_margin_events.push(event);
        }

        let (funding_timeslice_owed, event) =
            state.calc_capped_funding_payment(store, &position, starts_at, ends_at, false)?;
        if let Some(event) = event {
            insufficient_margin_events.push(event);
        }

        let funding_timeslice_owed = state.track_funding_fee_payment_with_capping(
            store,
            funding_timeslice_owed,
            &position,
        )?;

        position
            .funding_fee
            .checked_add_assign(funding_timeslice_owed.amount, &price)?;

        position.borrow_fee.checked_add_assign(
            (borrow_fee_timeslice_owed.lp + borrow_fee_timeslice_owed.xlp)?,
            &price,
        )?;

        // collect the borrow fee portion, which is ultimately paid out to liquidity providers
        // funding portion is paid between positions and dealt with through
        // the inherent position bookkeeping, not a global fee collection
        let borrow_fee_collection =
            state.collect_borrow_fee(store, position.id, borrow_fee_timeslice_owed, price)?;

        let market_type = state.market_id(store)?.get_market_type();

        let funding_payment_event = FundingPaymentEvent {
            pos_id: position.id,
            amount: funding_timeslice_owed.amount,
            amount_usd: funding_timeslice_owed
                .amount
                .map(|x| price.collateral_to_usd(x)),
            direction: position.direction().into_base(market_type),
        };

        let cap_crank_fee = if charge_crank_fee {
            //let (crank_fee, crank_fee_usd) =
            let cap_crank_fee =
                state.cap_crank_fee(&position, &price, state.config.crank_fee_charged)?;
            position
                .crank_fee
                .checked_add_assign(cap_crank_fee.amount, &price)?;
            Some(cap_crank_fee)
        } else {
            None
        };

        // Update the active collateral
        debug_assert!(position.active_collateral.raw() >= position.liquidation_margin.total()?);
        let to_subtract = (((borrow_fee_timeslice_owed.lp.into_signed()
            + borrow_fee_timeslice_owed.xlp.into_signed())?
            + funding_timeslice_owed.amount)?
            + cap_crank_fee
                .as_ref()
                .map(|crank_fee| crank_fee.amount.into_signed())
                .unwrap_or(Collateral::zero().into_signed()))?;
        debug_assert!(to_subtract <= position.liquidation_margin.total()?.into_signed());

        let position = match position
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
                    insufficient_margin_events.push(InsufficientMarginEvent {
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
                position.active_collateral = "0.000000001".parse().unwrap();
                MaybeClosedPosition::Close(ClosePositionInstructions {
                    pos: position,
                    capped_exposure: Signed::<Collateral>::zero(),
                    additional_losses: Collateral::zero(),
                    settlement_price: price,
                    reason: PositionCloseReason::Liquidated(LiquidationReason::Liquidated),
                    closed_during_liquifunding: true,
                })
            }
        };

        Ok(Self {
            position,
            insufficient_margin_events,
            funding_fee_payment: funding_timeslice_owed,
            borrow_fee_collection,
            funding_payment_event,
            cap_crank_fee,
        })
    }
    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<()> {
        for event in &self.insufficient_margin_events {
            ctx.response_mut().add_event(event);
        }

        self.funding_fee_payment.apply(state, ctx)?;
        self.borrow_fee_collection.apply(state, ctx)?;
        ctx.response_mut().add_event(&self.funding_payment_event);
        if let Some(cap_crank_fee) = self.cap_crank_fee {
            cap_crank_fee.apply(state, ctx)?;
        }
        Ok(())
    }
}

#[must_use]
pub(crate) struct FundingFeePaymentWithCapping {
    pub(crate) amount: Signed<Collateral>,
    pub(crate) insufficient_margin_event: Option<InsufficientMarginEvent>,
    pub(crate) total_net_funding_paid: Signed<Collateral>,
    #[cfg(debug_assertions)]
    pub(crate) margin_to_check: Collateral,
}

impl FundingFeePaymentWithCapping {
    pub(crate) fn apply(self, _state: &State, ctx: &mut StateContext) -> Result<()> {
        if let Some(event) = &self.insufficient_margin_event {
            ctx.response_mut().add_event(event);
        }
        TOTAL_NET_FUNDING_PAID.save(ctx.storage, &self.total_net_funding_paid)?;

        #[cfg(debug_assertions)]
        debug_check_invariants(ctx.storage, self.margin_to_check);

        Ok(())
    }
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
        .unwrap()
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
