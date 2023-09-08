//! Data structures and events for positions
mod closed;
mod collateral_and_usd;

pub use closed::*;
pub use collateral_and_usd::*;

use anyhow::Result;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal256, StdResult};
use cw_storage_plus::{IntKey, Key, KeyDeserialize, Prefixer, PrimaryKey};
use shared::prelude::*;
use std::fmt;
use std::hash::Hash;
use std::num::ParseIntError;
use std::str::FromStr;

use super::config::Config;

/// The position itself
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Position {
    /// Owner of the position
    pub owner: Addr,
    /// Unique identifier for a position
    pub id: PositionId,
    /// The amount of collateral deposited by the trader to create this position.
    ///
    /// It would seem like the type here should be `NonZero<Collateral>`.
    /// However, due to updates, this isn't accurate. It's possible for someone
    /// to update a position and withdraw more collateral than the original
    /// deposit.
    pub deposit_collateral: SignedCollateralAndUsd,
    /// Active collateral for the position
    ///
    /// As a position stays open, we liquifund to realize price exposure and
    /// take fees. This is the current trader-side collateral after those steps.
    pub active_collateral: NonZero<Collateral>,
    /// Collateral owned by the liquidity pool that is locked in this position.
    pub counter_collateral: NonZero<Collateral>,
    /// This is signed, where negative represents a short and positive is a long
    pub notional_size: Signed<Notional>,
    /// When the position was created.
    pub created_at: Timestamp,
    /// The one-time fee paid when opening or updating a position
    ///
    /// this value is the current balance, including all updates
    pub trading_fee: CollateralAndUsd,
    /// The ongoing fee paid (and earned!) between positions
    /// to incentivize keeping longs and shorts in balance
    /// which in turn reduces risk for LPs
    ///
    /// This value is the current balance, not a historical record of each payment
    pub funding_fee: SignedCollateralAndUsd,
    /// The ongoing fee paid to LPs to lock up their deposit
    /// as counter-size collateral in this position
    ///
    /// This value is the current balance, not a historical record of each payment
    pub borrow_fee: CollateralAndUsd,

    /// Total crank fees paid
    pub crank_fee: CollateralAndUsd,

    /// Cumulative amount of delta neutrality fees paid by (or received by) the position.
    ///
    /// Positive == outgoing, negative == incoming, like funding_fee.
    pub delta_neutrality_fee: SignedCollateralAndUsd,

    /// Last time the position was liquifunded.
    ///
    /// For newly opened positions, this is the same as the creation time.
    pub liquifunded_at: Timestamp,
    /// When is our next scheduled liquifunding?
    ///
    /// The crank will automatically liquifund this position once this timestamp
    /// has passed. Additionally, liquifunding may be triggered by updating the
    /// position.
    pub next_liquifunding: Timestamp,
    /// At what point will this position be stale? Staleness means we cannot
    /// guarantee sufficient liquidation margin is present to cover fees.
    pub stale_at: Timestamp,
    /// A trader specified price at which the position will be liquidated
    pub stop_loss_override: Option<PriceBaseInQuote>,
    /// Stored separately to ensure there are no rounding errors, since we need precise binary equivalence for lookups.
    pub stop_loss_override_notional: Option<Price>,
    /// A trader specified price at which the position will be closed in profit
    pub take_profit_override: Option<PriceBaseInQuote>,
    /// Stored separately to ensure there are no rounding errors, since we need precise binary equivalence for lookups.
    pub take_profit_override_notional: Option<Price>,
    /// The most recently calculated liquidation price
    pub liquidation_price: Option<Price>,
    /// The most recently calculated take profit (max gains) price
    pub take_profit_price: Option<Price>,
    /// The amount of liquidation margin set aside
    pub liquidation_margin: LiquidationMargin,
}

/// Liquidation margin for a position, broken down by component.
///
/// Each field represents how much collateral has been set aside for the given
/// fees, or the maximum amount the position can pay at liquifunding.
#[cw_serde]
#[derive(Default, Copy, Eq)]
pub struct LiquidationMargin {
    /// Maximum borrow fee payment.
    pub borrow: Collateral,
    /// Maximum funding payment.
    pub funding: Collateral,
    /// Maximum delta neutrality fee.
    pub delta_neutrality: Collateral,
    /// Funds set aside for a single crank fee.
    pub crank: Collateral,
}

impl LiquidationMargin {
    /// Total value of the liquidation margin fields
    pub fn total(&self) -> Collateral {
        self.borrow + self.funding + self.delta_neutrality + self.crank
    }
}

/// Response from [QueryMsg::Positions]
#[cw_serde]
pub struct PositionsResp {
    /// Open positions
    pub positions: Vec<PositionQueryResponse>,
    /// Positions which are pending a liquidation/take profit
    ///
    /// The closed position information is not the final version of the data,
    /// the close process itself still needs to make final payments.
    pub pending_close: Vec<ClosedPosition>,
    /// Positions which have already been closed.
    pub closed: Vec<ClosedPosition>,
}

/// Query response representing current state of a position
#[cw_serde]
pub struct PositionQueryResponse {
    /// Owner
    pub owner: Addr,
    /// Unique ID
    pub id: PositionId,
    /// Direction
    pub direction_to_base: DirectionToBase,
    /// Current leverage
    ///
    /// This is impacted by fees and price exposure
    pub leverage: LeverageToBase,
    /// Leverage of the counter collateral
    pub counter_leverage: LeverageToBase,
    /// When the position was opened
    pub created_at: Timestamp,
    /// When the position was last liquifunded
    pub liquifunded_at: Timestamp,

    /// The one-time fee paid when opening or updating a position
    ///
    /// This value is the current balance, including all updates
    pub trading_fee_collateral: Collateral,
    /// USD expression of [Self::trading_fee_collateral] using cost-basis calculation.
    pub trading_fee_usd: Usd,
    /// The ongoing fee paid (and earned!) between positions
    /// to incentivize keeping longs and shorts in balance
    /// which in turn reduces risk for LPs
    ///
    /// This value is the current balance, not a historical record of each payment
    pub funding_fee_collateral: Signed<Collateral>,
    /// USD expression of [Self::funding_fee_collateral] using cost-basis calculation.
    pub funding_fee_usd: Signed<Usd>,
    /// The ongoing fee paid to LPs to lock up their deposit
    /// as counter-size collateral in this position
    ///
    /// This value is the current balance, not a historical record of each payment
    pub borrow_fee_collateral: Collateral,
    /// USD expression of [Self::borrow_fee_collateral] using cost-basis calculation.
    pub borrow_fee_usd: Usd,

    /// Cumulative amount of crank fees paid by the position
    pub crank_fee_collateral: Collateral,
    /// USD expression of [Self::crank_fee_collateral] using cost-basis calculation.
    pub crank_fee_usd: Usd,

    /// Aggregate delta neutrality fees paid or received through position opens and upates.
    pub delta_neutrality_fee_collateral: Signed<Collateral>,
    /// USD expression of [Self::delta_neutrality_fee_collateral] using cost-basis calculation.
    pub delta_neutrality_fee_usd: Signed<Usd>,

    /// See [Position::deposit_collateral]
    pub deposit_collateral: Signed<Collateral>,
    /// USD expression of [Self::deposit_collateral] using cost-basis calculation.
    pub deposit_collateral_usd: Signed<Usd>,
    /// See [Position::active_collateral]
    pub active_collateral: NonZero<Collateral>,
    /// [Self::active_collateral] converted to USD at the current exchange rate
    pub active_collateral_usd: NonZero<Usd>,
    /// See [Position::counter_collateral]
    pub counter_collateral: NonZero<Collateral>,

    /// Unrealized PnL on this position, in terms of collateral.
    pub pnl_collateral: Signed<Collateral>,
    /// Unrealized PnL on this position, in USD, using cost-basis analysis.
    pub pnl_usd: Signed<Usd>,

    /// DNF that would be charged (positive) or received (negative) if position was closed now.
    pub dnf_on_close_collateral: Signed<Collateral>,

    /// Notional size of the position
    pub notional_size: Signed<Notional>,
    /// Notional size converted to collateral at the current price
    pub notional_size_in_collateral: Signed<Collateral>,

    /// The size of the position in terms of the base asset.
    ///
    /// Note that this is not a simple conversion from notional size. Instead,
    /// this needs to account for the off-by-one leverage that occurs in
    /// collateral-is-base markets.
    pub position_size_base: Signed<Base>,

    /// Convert [Self::position_size_base] into USD at the current exchange rate.
    pub position_size_usd: Signed<Usd>,

    /// Price at which liquidation will occur
    pub liquidation_price_base: Option<PriceBaseInQuote>,
    /// The liquidation margin set aside on this position
    pub liquidation_margin: LiquidationMargin,

    /// Maximum gains, in terms of quote, the trader can achieve
    pub max_gains_in_quote: MaxGainsInQuote,
    /// Price at which trader will achieve maximum gains and take all counter collateral.
    pub take_profit_price_base: Option<PriceBaseInQuote>,

    /// Entry price
    pub entry_price_base: PriceBaseInQuote,

    /// When the next liquifunding is scheduled
    pub next_liquifunding: Timestamp,
    /// Point at which this position will be stale if not liquifunded
    pub stale_at: Timestamp,

    /// Stop loss price set by the trader
    pub stop_loss_override: Option<PriceBaseInQuote>,
    /// Take profit price set by the trader
    pub take_profit_override: Option<PriceBaseInQuote>,
}

impl Position {
    /// Direction of the position
    pub fn direction(&self) -> DirectionToNotional {
        if self.notional_size.is_negative() {
            DirectionToNotional::Short
        } else {
            DirectionToNotional::Long
        }
    }

    /// Maximum gains for the position
    pub fn max_gains_in_quote(
        &self,
        market_type: MarketType,
        price_point: PricePoint,
    ) -> Result<MaxGainsInQuote> {
        match market_type {
            MarketType::CollateralIsQuote => Ok(MaxGainsInQuote::Finite(
                self.counter_collateral
                    .checked_div_collateral(self.active_collateral)?,
            )),
            MarketType::CollateralIsBase => {
                let take_profit_price = self.take_profit_price(&price_point, market_type)?;
                let take_profit_price = match take_profit_price {
                    Some(price) => price,
                    None => return Ok(MaxGainsInQuote::PosInfinity),
                };
                let take_profit_collateral = self
                    .active_collateral
                    .checked_add(self.counter_collateral.raw())?;
                let take_profit_in_notional =
                    take_profit_price.collateral_to_notional_non_zero(take_profit_collateral);
                let active_collateral_in_notional =
                    price_point.collateral_to_notional_non_zero(self.active_collateral);
                anyhow::ensure!(
                    take_profit_in_notional > active_collateral_in_notional,
                    "Max gains in quote is negative, this should not be possible.
                    Take profit: {take_profit_in_notional}.
                    Active collateral: {active_collateral_in_notional}"
                );
                let res = (take_profit_in_notional.into_decimal256()
                    - active_collateral_in_notional.into_decimal256())
                .checked_div(active_collateral_in_notional.into_decimal256())?;
                Ok(MaxGainsInQuote::Finite(
                    NonZero::new(res).context("Max gains of 0")?,
                ))
            }
        }
    }

    /// Compute the internal leverage active collateral.
    pub fn active_leverage_to_notional(
        &self,
        price_point: &PricePoint,
    ) -> SignedLeverageToNotional {
        SignedLeverageToNotional::calculate(self.notional_size, price_point, self.active_collateral)
    }

    /// Compute the internal leverage for the counter collateral.
    pub fn counter_leverage_to_notional(
        &self,
        price_point: &PricePoint,
    ) -> SignedLeverageToNotional {
        SignedLeverageToNotional::calculate(
            self.notional_size,
            price_point,
            self.counter_collateral,
        )
    }

    /// Convert the notional size into collateral at the given price point.
    pub fn notional_size_in_collateral(&self, price_point: &PricePoint) -> Signed<Collateral> {
        self.notional_size
            .map(|x| price_point.notional_to_collateral(x))
    }

    /// Calculate the size of the position in terms of the base asset.
    ///
    /// This represents what the users' perception of their position is, and
    /// needs to take into account the off-by-one leverage impact of
    /// collateral-is-base markets.
    pub fn position_size_base(
        &self,
        market_type: MarketType,
        price_point: &PricePoint,
    ) -> Result<Signed<Base>> {
        let leverage = self
            .active_leverage_to_notional(price_point)
            .into_base(market_type);
        let active_collateral = price_point.collateral_to_base_non_zero(self.active_collateral);
        leverage.checked_mul_base(active_collateral)
    }

    /// Calculate the PnL of this position in terms of the collateral.
    pub fn pnl_in_collateral(&self) -> Signed<Collateral> {
        self.active_collateral.into_signed() - self.deposit_collateral.collateral()
    }

    /// Calculate the PnL of this position in terms of USD.
    ///
    /// Note that this is not equivalent to converting the collateral PnL into
    /// USD, since we follow a cost basis model in this function, tracking the
    /// price of the collateral asset in terms of USD for each transaction.
    pub fn pnl_in_usd(&self, price_point: &PricePoint) -> Signed<Usd> {
        let active_collateral_in_usd =
            price_point.collateral_to_usd_non_zero(self.active_collateral);
        active_collateral_in_usd.into_signed() - self.deposit_collateral.usd()
    }

    /// Computes the liquidation margin for the position
    ///
    /// `price` is the price at the last liquifunding.
    ///
    /// `current_price_point` is used for converting fees from USD to collateral.
    pub fn liquidation_margin(
        &self,
        price: Price,
        current_price_point: &PricePoint,
        config: &Config,
    ) -> Result<LiquidationMargin> {
        const SEC_PER_YEAR: u64 = 31_536_000;
        const MS_PER_YEAR: u64 = SEC_PER_YEAR * 1000;
        // Panicking is fine here, it's a hard-coded value
        let ms_per_year = Decimal256::from_atomics(MS_PER_YEAR, 0).unwrap();

        let duration = config.liquidation_margin_duration().as_ms_decimal_lossy();

        let borrow_fee_max_rate =
            config.borrow_fee_rate_max_annualized.raw() * duration / ms_per_year;
        let borrow_fee_max_payment = (self
            .active_collateral
            .raw()
            .checked_add(self.counter_collateral.raw())?)
        .checked_mul_dec(borrow_fee_max_rate)?;

        let max_price = match self.direction() {
            DirectionToNotional::Long => {
                price.into_decimal256()
                    + self.counter_collateral.into_decimal256()
                        / self.notional_size.abs_unsigned().into_decimal256()
            }
            DirectionToNotional::Short => {
                price.into_decimal256()
                    + self.active_collateral.into_decimal256()
                        / self.notional_size.abs_unsigned().into_decimal256()
            }
        };

        let funding_max_rate = config.funding_rate_max_annualized * duration / ms_per_year;
        let funding_max_payment =
            funding_max_rate * self.notional_size.abs_unsigned().into_decimal256() * max_price;

        let slippage_max = config.delta_neutrality_fee_cap.into_decimal256()
            * self.notional_size.abs_unsigned().into_decimal256()
            * max_price;

        Ok(LiquidationMargin {
            borrow: borrow_fee_max_payment,
            funding: Collateral::from_decimal256(funding_max_payment),
            delta_neutrality: Collateral::from_decimal256(slippage_max),
            crank: current_price_point.usd_to_collateral(config.crank_fee_charged),
        })
    }

    /// Computes the liquidation price for the position at a given spot price.
    pub fn liquidation_price(
        &self,
        price: Price,
        active_collateral: NonZero<Collateral>,
        liquidation_margin: &LiquidationMargin,
    ) -> Option<Price> {
        let liquidation_price = price.into_number()
            - (active_collateral.into_number() - liquidation_margin.total().into_number())
                / self.notional_size.into_number();
        Price::try_from_number(liquidation_price).ok()
    }

    /// Computes the take-profit price for the position at a given spot price.
    pub fn take_profit_price(
        &self,
        price_point: &PricePoint,
        market_type: MarketType,
    ) -> Result<Option<Price>> {
        let take_profit_price_raw = price_point.price_notional.into_number().checked_add(
            self.counter_collateral
                .into_number()
                .checked_div(self.notional_size.into_number())?,
        )?;

        let take_profit_price = if take_profit_price_raw.approx_eq(Number::ZERO) {
            None
        } else {
            debug_assert!(
                take_profit_price_raw.is_positive_or_zero(),
                "There should never be a calculated take profit price which is negative. In production, this is treated as 0 to indicate infinite max gains."
            );
            Price::try_from_number(take_profit_price_raw).ok()
        };

        match take_profit_price {
            Some(price) => Ok(Some(price)),
            None =>
            match market_type {
                // Infinite max gains results in a notional take profit price of 0
                MarketType::CollateralIsBase => Ok(None),
                MarketType::CollateralIsQuote => Err(anyhow!("Calculated a take profit price of {take_profit_price_raw} in a collateral-is-quote market. Spot notional price: {}. Counter collateral: {}. Notional size: {}.", price_point.price_notional, self.counter_collateral,self.notional_size)),
            }
        }
    }

    /// Add a new delta neutrality fee to the position.
    pub fn add_delta_neutrality_fee(
        &mut self,
        amount: Signed<Collateral>,
        price_point: &PricePoint,
    ) -> Result<()> {
        self.delta_neutrality_fee
            .checked_add_assign(amount, price_point)
    }

    /// Apply a price change to this position.
    ///
    /// This will determine the exposure (positive == to trader, negative == to
    /// liquidity pool) impact and return whether the position can remain open
    /// or instructions to close it.
    pub fn settle_price_exposure(
        mut self,
        start_price: Price,
        end_price: Price,
        liquidation_margin: Collateral,
        ends_at: Timestamp,
    ) -> Result<(MaybeClosedPosition, Signed<Collateral>)> {
        let price_delta = end_price.into_number() - start_price.into_number();
        let exposure =
            Signed::<Collateral>::from_number(price_delta * self.notional_size.into_number());
        let min_exposure = liquidation_margin
            .into_signed()
            .checked_sub(self.active_collateral.into_signed())?;
        let max_exposure = self.counter_collateral.into_signed();

        Ok(if exposure <= min_exposure {
            (
                MaybeClosedPosition::Close(ClosePositionInstructions {
                    pos: self,
                    exposure: min_exposure,
                    close_time: ends_at,
                    settlement_time: ends_at,
                    reason: PositionCloseReason::Liquidated(LiquidationReason::Liquidated),
                }),
                min_exposure,
            )
        } else if exposure >= max_exposure {
            (
                MaybeClosedPosition::Close(ClosePositionInstructions {
                    pos: self,
                    exposure: max_exposure,
                    close_time: ends_at,
                    settlement_time: ends_at,
                    reason: PositionCloseReason::Liquidated(LiquidationReason::MaxGains),
                }),
                max_exposure,
            )
        } else {
            self.active_collateral = self.active_collateral.checked_add_signed(exposure)?;
            self.counter_collateral = self.counter_collateral.checked_sub_signed(exposure)?;
            (MaybeClosedPosition::Open(self), exposure)
        })
    }

    /// Convert a position into a query response, calculating price exposure impact.
    #[allow(clippy::too_many_arguments)]
    pub fn into_query_response_extrapolate_exposure(
        self,
        start_price: Price,
        end_price: PricePoint,
        entry_price: Price,
        current_price_point: &PricePoint,
        config: &Config,
        market_type: MarketType,
        original_direction_to_base: DirectionToBase,
        dnf_on_close_collateral: Signed<Collateral>,
    ) -> Result<PositionOrPendingClose> {
        // We always use the current spot price for the current_price_point
        // parameter to liquidation_margin. It's used exclusively to calculate
        // the crank fee, and therefore does not need to be based on the
        // liquifunding cadence.
        let liquidation_margin =
            self.liquidation_margin(start_price, current_price_point, config)?;

        let (settle_price_result, _exposure) = self.settle_price_exposure(
            start_price,
            end_price.price_notional,
            liquidation_margin.total(),
            end_price.timestamp,
        )?;

        let result = match settle_price_result {
            MaybeClosedPosition::Open(pos) => {
                // PERP-996 ensure we do not flip direction, see comments in
                // liquifunding for more details
                let new_direction_to_base = pos
                    .active_leverage_to_notional(&end_price)
                    .into_base(market_type)
                    .split()
                    .0;
                if original_direction_to_base == new_direction_to_base {
                    MaybeClosedPosition::Open(pos)
                } else {
                    MaybeClosedPosition::Close(ClosePositionInstructions {
                        pos,
                        exposure: Signed::zero(),
                        close_time: end_price.timestamp,
                        settlement_time: end_price.timestamp,
                        reason: PositionCloseReason::Liquidated(LiquidationReason::MaxGains),
                    })
                }
            }
            MaybeClosedPosition::Close(x) => MaybeClosedPosition::Close(x),
        };

        match result {
            MaybeClosedPosition::Open(position) => position
                .into_query_response(end_price, entry_price, market_type, dnf_on_close_collateral)
                .map(|pos| PositionOrPendingClose::Open(Box::new(pos))),
            MaybeClosedPosition::Close(ClosePositionInstructions {
                pos,
                exposure,
                close_time,
                settlement_time,
                reason,
            }) => {
                // Best effort closed position value
                let direction_to_base = pos.direction().into_base(market_type);
                let entry_price_base = entry_price.into_base_price(market_type);

                // Figure out the final active collateral.
                let active_collateral = if exposure > pos.counter_collateral.into_signed() {
                    // If exposure is greater than counter collateral, then take
                    // all the counter collateral.
                    pos.active_collateral
                        .raw()
                        .checked_add(pos.counter_collateral.raw())?
                } else {
                    // Otherwise, add in the total exposure value. If we go
                    // negative, set active collateral to 0.
                    pos.active_collateral
                        .raw()
                        .checked_add_signed(exposure)
                        .ok()
                        .unwrap_or_default()
                };
                let active_collateral_usd = end_price.collateral_to_usd(active_collateral);
                Ok(PositionOrPendingClose::PendingClose(Box::new(
                    ClosedPosition {
                        owner: pos.owner,
                        id: pos.id,
                        direction_to_base,
                        created_at: pos.created_at,
                        liquifunded_at: pos.liquifunded_at,
                        trading_fee_collateral: pos.trading_fee.collateral(),
                        trading_fee_usd: pos.trading_fee.usd(),
                        funding_fee_collateral: pos.funding_fee.collateral(),
                        funding_fee_usd: pos.funding_fee.usd(),
                        borrow_fee_collateral: pos.borrow_fee.collateral(),
                        borrow_fee_usd: pos.borrow_fee.usd(),
                        crank_fee_collateral: pos.crank_fee.collateral(),
                        crank_fee_usd: pos.crank_fee.usd(),
                        deposit_collateral: pos.deposit_collateral.collateral(),
                        deposit_collateral_usd: pos.deposit_collateral.usd(),
                        pnl_collateral: active_collateral
                            .into_signed()
                            .checked_sub(pos.deposit_collateral.collateral())?,
                        pnl_usd: active_collateral_usd
                            .into_signed()
                            .checked_sub(pos.deposit_collateral.usd())?,
                        notional_size: pos.notional_size,
                        entry_price_base,
                        close_time,
                        settlement_time,
                        reason,
                        active_collateral,
                        delta_neutrality_fee_collateral: pos.delta_neutrality_fee.collateral(),
                        delta_neutrality_fee_usd: pos.delta_neutrality_fee.usd(),
                    },
                )))
            }
        }
    }

    /// Convert into a query response, without calculating price exposure impact.
    pub fn into_query_response(
        self,
        end_price: PricePoint,
        entry_price: Price,
        market_type: MarketType,
        dnf_on_close_collateral: Signed<Collateral>,
    ) -> Result<PositionQueryResponse> {
        let (direction_to_base, leverage) = self
            .active_leverage_to_notional(&end_price)
            .into_base(market_type)
            .split();
        let counter_leverage = self
            .counter_leverage_to_notional(&end_price)
            .into_base(market_type)
            .split()
            .1;
        let pnl_collateral = self.pnl_in_collateral();
        let pnl_usd = self.pnl_in_usd(&end_price);
        let max_gains_in_quote = self.max_gains_in_quote(market_type, end_price)?;
        let notional_size_in_collateral = self.notional_size_in_collateral(&end_price);
        let position_size_base = self.position_size_base(market_type, &end_price)?;

        let Self {
            owner,
            id,
            active_collateral,
            deposit_collateral,
            counter_collateral,
            notional_size,
            created_at,
            trading_fee,
            funding_fee,
            borrow_fee,
            crank_fee,
            delta_neutrality_fee,
            liquifunded_at,
            next_liquifunding,
            stale_at,
            stop_loss_override,
            take_profit_override,
            liquidation_margin,
            liquidation_price,
            take_profit_price: take_profit,
            stop_loss_override_notional: _,
            take_profit_override_notional: _,
        } = self;

        Ok(PositionQueryResponse {
            owner,
            id,
            created_at,
            liquifunded_at,
            direction_to_base,
            leverage,
            counter_leverage,
            trading_fee_collateral: trading_fee.collateral(),
            trading_fee_usd: trading_fee.usd(),
            funding_fee_collateral: funding_fee.collateral(),
            funding_fee_usd: funding_fee.usd(),
            borrow_fee_collateral: borrow_fee.collateral(),
            borrow_fee_usd: borrow_fee.usd(),
            delta_neutrality_fee_collateral: delta_neutrality_fee.collateral(),
            delta_neutrality_fee_usd: delta_neutrality_fee.usd(),
            active_collateral,
            active_collateral_usd: end_price.collateral_to_usd_non_zero(active_collateral),
            deposit_collateral: deposit_collateral.collateral(),
            deposit_collateral_usd: deposit_collateral.usd(),
            pnl_collateral,
            pnl_usd,
            dnf_on_close_collateral,
            notional_size,
            notional_size_in_collateral,
            position_size_base,
            position_size_usd: position_size_base.map(|x| end_price.base_to_usd(x)),
            counter_collateral,
            max_gains_in_quote,
            liquidation_price_base: liquidation_price.map(|x| x.into_base_price(market_type)),
            liquidation_margin,
            take_profit_price_base: take_profit.map(|x| x.into_base_price(market_type)),
            entry_price_base: entry_price.into_base_price(market_type),
            next_liquifunding,
            stale_at,
            stop_loss_override,
            take_profit_override,
            crank_fee_collateral: crank_fee.collateral(),
            crank_fee_usd: crank_fee.usd(),
        })
    }

    /// Attributes for a position which can be emitted in events.
    pub fn attributes(&self) -> Vec<(&'static str, String)> {
        let LiquidationMargin {
            borrow: borrow_fee_max,
            funding: funding_max,
            delta_neutrality: slippage_max,
            crank,
        } = &self.liquidation_margin;
        vec![
            ("pos-owner", self.owner.to_string()),
            ("pos-id", self.id.to_string()),
            ("pos-active-collateral", self.active_collateral.to_string()),
            (
                "pos-deposit-collateral",
                self.deposit_collateral.collateral().to_string(),
            ),
            (
                "pos-deposit-collateral-usd",
                self.deposit_collateral.usd().to_string(),
            ),
            ("pos-trading-fee", self.trading_fee.collateral().to_string()),
            ("pos-trading-fee-usd", self.trading_fee.usd().to_string()),
            ("pos-crank-fee", self.crank_fee.collateral().to_string()),
            ("pos-crank-fee-usd", self.crank_fee.usd().to_string()),
            (
                "pos-counter-collateral",
                self.counter_collateral.to_string(),
            ),
            ("pos-notional-size", self.notional_size.to_string()),
            ("pos-created-at", self.created_at.to_string()),
            ("pos-liquifunded-at", self.liquifunded_at.to_string()),
            ("pos-next-liquifunding", self.next_liquifunding.to_string()),
            ("pos-stale-at", self.stale_at.to_string()),
            (
                "pos-borrow-fee-liquidation-margin",
                borrow_fee_max.to_string(),
            ),
            ("pos-funding-liquidation-margin", funding_max.to_string()),
            ("pos-slippage-liquidation-margin", slippage_max.to_string()),
            ("pos-crank-liquidation-margin", crank.to_string()),
        ]
    }
}

/// PositionId
#[cw_serde]
#[derive(Copy, PartialOrd, Ord, Eq)]
pub struct PositionId(Uint64);

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for PositionId {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        u64::arbitrary(u).map(PositionId::new)
    }
}

#[allow(clippy::derived_hash_with_manual_eq)]
impl Hash for PositionId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.u64().hash(state);
    }
}

impl PositionId {
    /// Construct a new value from a [u64].
    pub fn new(x: u64) -> Self {
        PositionId(x.into())
    }

    /// The underlying `u64` representation.
    pub fn u64(self) -> u64 {
        self.0.u64()
    }
}

impl<'a> PrimaryKey<'a> for PositionId {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.u64().to_cw_bytes())]
    }
}

impl<'a> Prefixer<'a> for PositionId {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.u64().to_cw_bytes())]
    }
}

impl KeyDeserialize for PositionId {
    type Output = PositionId;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        u64::from_vec(value).map(|x| PositionId(Uint64::new(x)))
    }
}

impl fmt::Display for PositionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for PositionId {
    type Err = ParseIntError;
    fn from_str(src: &str) -> Result<Self, ParseIntError> {
        src.parse().map(|x| PositionId(Uint64::new(x)))
    }
}

/// Events
pub mod events {
    use super::*;
    use crate::constants::{event_key, event_val};
    use cosmwasm_std::Event;

    /// Collaterals calculated on a position.
    #[cw_serde]
    pub struct PositionCollaterals {
        /// [PositionQueryResponse::deposit_collateral]
        pub deposit_collateral: Signed<Collateral>,
        /// [PositionQueryResponse::deposit_collateral_usd]
        pub deposit_collateral_usd: Signed<Usd>,
        /// [PositionQueryResponse::active_collateral]
        pub active_collateral: NonZero<Collateral>,
        /// [PositionQueryResponse::counter_collateral]
        pub counter_collateral: NonZero<Collateral>,
    }

    /// Trading fees paid for a position
    #[cw_serde]
    pub struct PositionTradingFee {
        /// In collateral
        pub trading_fee: Collateral,
        /// In USD
        pub trading_fee_usd: Usd,
    }

    /// Returns a tuple containing (base, quote) calculated based on the market type
    pub fn calculate_base_and_quote(
        market_type: MarketType,
        price: Price,
        amount: Number,
    ) -> Result<(Number, Number)> {
        Ok(match market_type {
            MarketType::CollateralIsQuote => (amount.checked_div(price.into_number())?, amount),
            MarketType::CollateralIsBase => (amount, amount.checked_mul(price.into_number())?),
        })
    }

    /// Calculate the collaterals for a position
    pub fn calculate_position_collaterals(pos: &Position) -> Result<PositionCollaterals> {
        Ok(PositionCollaterals {
            deposit_collateral: pos.deposit_collateral.collateral(),
            deposit_collateral_usd: pos.deposit_collateral.usd(),
            active_collateral: pos.active_collateral,
            counter_collateral: pos.counter_collateral,
        })
    }

    /// All attributes for a position
    #[cw_serde]
    pub struct PositionAttributes {
        /// [Position::id]
        pub pos_id: PositionId,
        /// [Position::owner]
        pub owner: Addr,
        /// Collaterals calculated on the position
        pub collaterals: PositionCollaterals,
        /// Type of the market it was opened in
        pub market_type: MarketType,
        /// [Position::notional_size]
        pub notional_size: Signed<Notional>,
        /// [PositionQueryResponse::notional_size_in_collateral]
        pub notional_size_in_collateral: Signed<Collateral>,
        /// Calculated using the exchange rate when the event was emitted
        pub notional_size_usd: Signed<Usd>,
        /// Trading fee
        pub trading_fee: PositionTradingFee,
        /// Direction
        pub direction: DirectionToBase,
        /// Trader leverage
        pub leverage: LeverageToBase,
        /// Counter leverage
        pub counter_leverage: LeverageToBase,
        /// Stop loss price
        pub stop_loss_override: Option<PriceBaseInQuote>,
        /// Take profit price
        pub take_profit_override: Option<PriceBaseInQuote>,
    }

    impl PositionAttributes {
        fn add_to_event(&self, event: &Event) -> Event {
            let mut event = event
                .clone()
                .add_attribute(event_key::POS_ID, self.pos_id.to_string())
                .add_attribute(event_key::POS_OWNER, self.owner.clone())
                .add_attribute(
                    event_key::DEPOSIT_COLLATERAL,
                    self.collaterals.deposit_collateral.to_string(),
                )
                .add_attribute(
                    event_key::DEPOSIT_COLLATERAL_USD,
                    self.collaterals.deposit_collateral_usd.to_string(),
                )
                .add_attribute(
                    event_key::ACTIVE_COLLATERAL,
                    self.collaterals.active_collateral.to_string(),
                )
                .add_attribute(
                    event_key::COUNTER_COLLATERAL,
                    self.collaterals.counter_collateral.to_string(),
                )
                .add_attribute(
                    event_key::MARKET_TYPE,
                    match self.market_type {
                        MarketType::CollateralIsQuote => event_val::NOTIONAL_BASE,
                        MarketType::CollateralIsBase => event_val::COLLATERAL_BASE,
                    },
                )
                .add_attribute(event_key::NOTIONAL_SIZE, self.notional_size.to_string())
                .add_attribute(
                    event_key::NOTIONAL_SIZE_IN_COLLATERAL,
                    self.notional_size_in_collateral.to_string(),
                )
                .add_attribute(
                    event_key::NOTIONAL_SIZE_USD,
                    self.notional_size_usd.to_string(),
                )
                .add_attribute(
                    event_key::TRADING_FEE,
                    self.trading_fee.trading_fee.to_string(),
                )
                .add_attribute(
                    event_key::TRADING_FEE_USD,
                    self.trading_fee.trading_fee_usd.to_string(),
                )
                .add_attribute(event_key::DIRECTION, self.direction.as_str())
                .add_attribute(event_key::LEVERAGE, self.leverage.to_string())
                .add_attribute(
                    event_key::COUNTER_LEVERAGE,
                    self.counter_leverage.to_string(),
                );

            if let Some(stop_loss_override) = self.stop_loss_override {
                event = event.add_attribute(
                    event_key::STOP_LOSS_OVERRIDE,
                    stop_loss_override.to_string(),
                );
            }

            if let Some(take_profit_override) = self.take_profit_override {
                event = event.add_attribute(
                    event_key::TAKE_PROFIT_OVERRIDE,
                    take_profit_override.to_string(),
                );
            }

            event
        }
    }

    impl TryFrom<Event> for PositionAttributes {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(Self {
                pos_id: PositionId::new(evt.u64_attr(event_key::POS_ID)?),
                owner: evt.unchecked_addr_attr(event_key::POS_OWNER)?,
                collaterals: PositionCollaterals {
                    deposit_collateral: evt.number_attr(event_key::DEPOSIT_COLLATERAL)?,
                    deposit_collateral_usd: evt.number_attr(event_key::DEPOSIT_COLLATERAL_USD)?,
                    active_collateral: evt.non_zero_attr(event_key::ACTIVE_COLLATERAL)?,
                    counter_collateral: evt.non_zero_attr(event_key::COUNTER_COLLATERAL)?,
                },
                market_type: evt.map_attr_result(event_key::MARKET_TYPE, |s| match s {
                    event_val::NOTIONAL_BASE => Ok(MarketType::CollateralIsQuote),
                    event_val::COLLATERAL_BASE => Ok(MarketType::CollateralIsBase),
                    _ => Err(PerpError::unimplemented().into()),
                })?,
                notional_size: evt.number_attr(event_key::NOTIONAL_SIZE)?,
                notional_size_in_collateral: evt
                    .number_attr(event_key::NOTIONAL_SIZE_IN_COLLATERAL)?,
                notional_size_usd: evt.number_attr(event_key::NOTIONAL_SIZE_USD)?,
                trading_fee: PositionTradingFee {
                    trading_fee: evt.decimal_attr(event_key::TRADING_FEE)?,
                    trading_fee_usd: evt.decimal_attr(event_key::TRADING_FEE_USD)?,
                },
                direction: evt.direction_attr(event_key::DIRECTION)?,
                leverage: evt.leverage_to_base_attr(event_key::LEVERAGE)?,
                counter_leverage: evt.leverage_to_base_attr(event_key::COUNTER_LEVERAGE)?,
                stop_loss_override: match evt.try_number_attr(event_key::STOP_LOSS_OVERRIDE)? {
                    None => None,
                    Some(stop_loss_override) => {
                        Some(PriceBaseInQuote::try_from_number(stop_loss_override)?)
                    }
                },
                take_profit_override: match evt.try_number_attr(event_key::TAKE_PROFIT_OVERRIDE)? {
                    None => None,
                    Some(take_profit_override) => {
                        Some(PriceBaseInQuote::try_from_number(take_profit_override)?)
                    }
                },
            })
        }
    }

    /// A position was closed
    #[derive(Debug, Clone)]
    pub struct PositionCloseEvent {
        /// Details on the closed position
        pub closed_position: ClosedPosition,
    }

    impl PerpEvent for PositionCloseEvent {}
    impl From<PositionCloseEvent> for Event {
        fn from(
            PositionCloseEvent {
                closed_position:
                    ClosedPosition {
                        owner,
                        id,
                        direction_to_base,
                        created_at,
                        liquifunded_at,
                        trading_fee_collateral,
                        trading_fee_usd,
                        funding_fee_collateral,
                        funding_fee_usd,
                        borrow_fee_collateral,
                        borrow_fee_usd,
                        crank_fee_collateral,
                        crank_fee_usd,
                        delta_neutrality_fee_collateral,
                        delta_neutrality_fee_usd,
                        deposit_collateral,
                        deposit_collateral_usd,
                        active_collateral,
                        pnl_collateral,
                        pnl_usd,
                        notional_size,
                        entry_price_base,
                        close_time,
                        settlement_time,
                        reason,
                    },
            }: PositionCloseEvent,
        ) -> Self {
            Event::new(event_key::POSITION_CLOSE)
                .add_attribute(event_key::POS_OWNER, owner.to_string())
                .add_attribute(event_key::POS_ID, id.to_string())
                .add_attribute(event_key::DIRECTION, direction_to_base.as_str())
                .add_attribute(event_key::CREATED_AT, created_at.to_string())
                .add_attribute(event_key::LIQUIFUNDED_AT, liquifunded_at.to_string())
                .add_attribute(event_key::TRADING_FEE, trading_fee_collateral.to_string())
                .add_attribute(event_key::TRADING_FEE_USD, trading_fee_usd.to_string())
                .add_attribute(event_key::FUNDING_FEE, funding_fee_collateral.to_string())
                .add_attribute(event_key::FUNDING_FEE_USD, funding_fee_usd.to_string())
                .add_attribute(event_key::BORROW_FEE, borrow_fee_collateral.to_string())
                .add_attribute(event_key::BORROW_FEE_USD, borrow_fee_usd.to_string())
                .add_attribute(
                    event_key::DELTA_NEUTRALITY_FEE,
                    delta_neutrality_fee_collateral.to_string(),
                )
                .add_attribute(
                    event_key::DELTA_NEUTRALITY_FEE_USD,
                    delta_neutrality_fee_usd.to_string(),
                )
                .add_attribute(event_key::CRANK_FEE, crank_fee_collateral.to_string())
                .add_attribute(event_key::CRANK_FEE_USD, crank_fee_usd.to_string())
                .add_attribute(
                    event_key::DEPOSIT_COLLATERAL,
                    deposit_collateral.to_string(),
                )
                .add_attribute(
                    event_key::DEPOSIT_COLLATERAL_USD,
                    deposit_collateral_usd.to_string(),
                )
                .add_attribute(event_key::PNL, pnl_collateral.to_string())
                .add_attribute(event_key::PNL_USD, pnl_usd.to_string())
                .add_attribute(event_key::NOTIONAL_SIZE, notional_size.to_string())
                .add_attribute(event_key::ENTRY_PRICE, entry_price_base.to_string())
                .add_attribute(event_key::CLOSED_AT, close_time.to_string())
                .add_attribute(event_key::SETTLED_AT, settlement_time.to_string())
                .add_attribute(
                    event_key::CLOSE_REASON,
                    match reason {
                        PositionCloseReason::Liquidated(LiquidationReason::Liquidated) => {
                            event_val::LIQUIDATED
                        }
                        PositionCloseReason::Liquidated(LiquidationReason::MaxGains) => {
                            event_val::MAX_GAINS
                        }
                        PositionCloseReason::Liquidated(LiquidationReason::StopLoss) => {
                            event_val::STOP_LOSS
                        }
                        PositionCloseReason::Liquidated(LiquidationReason::TakeProfit) => {
                            event_val::TAKE_PROFIT
                        }
                        PositionCloseReason::Direct => event_val::DIRECT,
                    },
                )
                .add_attribute(event_key::ACTIVE_COLLATERAL, active_collateral.to_string())
        }
    }
    impl TryFrom<Event> for PositionCloseEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            let closed_position = ClosedPosition {
                close_time: evt.timestamp_attr(event_key::CLOSED_AT)?,
                settlement_time: evt.timestamp_attr(event_key::SETTLED_AT)?,
                reason: evt.map_attr_result(event_key::CLOSE_REASON, |s| match s {
                    event_val::LIQUIDATED => Ok(PositionCloseReason::Liquidated(
                        LiquidationReason::Liquidated,
                    )),
                    event_val::MAX_GAINS => {
                        Ok(PositionCloseReason::Liquidated(LiquidationReason::MaxGains))
                    }
                    event_val::STOP_LOSS => {
                        Ok(PositionCloseReason::Liquidated(LiquidationReason::StopLoss))
                    }
                    event_val::TAKE_PROFIT => Ok(PositionCloseReason::Liquidated(
                        LiquidationReason::TakeProfit,
                    )),
                    event_val::DIRECT => Ok(PositionCloseReason::Direct),
                    _ => Err(PerpError::unimplemented().into()),
                })?,
                owner: evt.unchecked_addr_attr(event_key::POS_OWNER)?,
                id: PositionId::new(evt.u64_attr(event_key::POS_ID)?),
                direction_to_base: evt.direction_attr(event_key::DIRECTION)?,
                created_at: evt.timestamp_attr(event_key::CREATED_AT)?,
                liquifunded_at: evt.timestamp_attr(event_key::LIQUIFUNDED_AT)?,
                trading_fee_collateral: evt.decimal_attr(event_key::TRADING_FEE)?,
                trading_fee_usd: evt.decimal_attr(event_key::TRADING_FEE_USD)?,
                funding_fee_collateral: evt.number_attr(event_key::FUNDING_FEE)?,
                funding_fee_usd: evt.number_attr(event_key::FUNDING_FEE_USD)?,
                borrow_fee_collateral: evt.decimal_attr(event_key::BORROW_FEE)?,
                borrow_fee_usd: evt.decimal_attr(event_key::BORROW_FEE_USD)?,
                crank_fee_collateral: evt.decimal_attr(event_key::CRANK_FEE)?,
                crank_fee_usd: evt.decimal_attr(event_key::CRANK_FEE_USD)?,
                delta_neutrality_fee_collateral: evt
                    .number_attr(event_key::DELTA_NEUTRALITY_FEE)?,
                delta_neutrality_fee_usd: evt.number_attr(event_key::DELTA_NEUTRALITY_FEE_USD)?,
                deposit_collateral: evt.number_attr(event_key::DEPOSIT_COLLATERAL)?,
                deposit_collateral_usd: evt
                    // For migrations, this data wasn't always present
                    .try_number_attr(event_key::DEPOSIT_COLLATERAL_USD)?
                    .unwrap_or_default(),
                pnl_collateral: evt.number_attr(event_key::PNL)?,
                pnl_usd: evt.number_attr(event_key::PNL_USD)?,
                notional_size: evt.number_attr(event_key::NOTIONAL_SIZE)?,
                entry_price_base: PriceBaseInQuote::try_from_number(
                    evt.number_attr(event_key::ENTRY_PRICE)?,
                )?,
                active_collateral: evt.decimal_attr(event_key::ACTIVE_COLLATERAL)?,
            };
            Ok(PositionCloseEvent { closed_position })
        }
    }

    /// A position was opened
    pub struct PositionOpenEvent {
        /// Details of the position
        pub position_attributes: PositionAttributes,
        /// When it was opened
        pub created_at: Timestamp,
    }

    impl PerpEvent for PositionOpenEvent {}
    impl From<PositionOpenEvent> for Event {
        fn from(src: PositionOpenEvent) -> Self {
            let event = Event::new(event_key::POSITION_OPEN)
                .add_attribute(event_key::CREATED_AT, src.created_at.to_string());

            src.position_attributes.add_to_event(&event)
        }
    }
    impl TryFrom<Event> for PositionOpenEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(Self {
                created_at: evt.timestamp_attr(event_key::CREATED_AT)?,
                position_attributes: evt.try_into()?,
            })
        }
    }

    /// Event when a position has been updated
    #[cw_serde]
    pub struct PositionUpdateEvent {
        /// Attributes about the position
        pub position_attributes: PositionAttributes,
        /// Amount of collateral added or removed to the position
        pub deposit_collateral_delta: Signed<Collateral>,
        /// [Self::deposit_collateral_delta] converted to USD at the current price.
        pub deposit_collateral_delta_usd: Signed<Usd>,
        /// Change to active collateral
        pub active_collateral_delta: Signed<Collateral>,
        /// [Self::active_collateral_delta] converted to USD at the current price.
        pub active_collateral_delta_usd: Signed<Usd>,
        /// Change to counter collateral
        pub counter_collateral_delta: Signed<Collateral>,
        /// [Self::counter_collateral_delta] converted to USD at the current price.
        pub counter_collateral_delta_usd: Signed<Usd>,
        /// Change in trader leverage
        pub leverage_delta: Signed<Decimal256>,
        /// Change in counter collateral leverage
        pub counter_leverage_delta: Signed<Decimal256>,
        /// Change in the notional size
        pub notional_size_delta: Signed<Notional>,
        /// [Self::notional_size_delta] converted to USD at the current price.
        pub notional_size_delta_usd: Signed<Usd>,
        /// The change in notional size from the absolute value
        ///
        /// not the absolute value of delta itself
        /// e.g. from -10 to -15 will be 5, because it's the delta of 15-10
        /// but -15 to -10 will be -5, because it's the delta of 10-15
        pub notional_size_abs_delta: Signed<Notional>,
        /// [Self::notional_size_abs_delta] converted to USD at the current price.
        pub notional_size_abs_delta_usd: Signed<Usd>,
        /// Additional trading fee paid
        pub trading_fee_delta: Collateral,
        /// [Self::trading_fee_delta] converted to USD at the current price.
        pub trading_fee_delta_usd: Usd,
        /// Additional delta neutrality fee paid (or received)
        pub delta_neutrality_fee_delta: Signed<Collateral>,
        /// [Self::delta_neutrality_fee_delta] converted to USD at the current price.
        pub delta_neutrality_fee_delta_usd: Signed<Usd>,
        /// When the update occurred
        pub updated_at: Timestamp,
    }

    impl PerpEvent for PositionUpdateEvent {}
    impl From<PositionUpdateEvent> for Event {
        fn from(
            PositionUpdateEvent {
                position_attributes,
                deposit_collateral_delta,
                deposit_collateral_delta_usd,
                active_collateral_delta,
                active_collateral_delta_usd,
                counter_collateral_delta,
                counter_collateral_delta_usd,
                leverage_delta,
                counter_leverage_delta,
                notional_size_delta,
                notional_size_delta_usd,
                notional_size_abs_delta,
                notional_size_abs_delta_usd,
                trading_fee_delta,
                trading_fee_delta_usd,
                delta_neutrality_fee_delta,
                delta_neutrality_fee_delta_usd,
                updated_at,
            }: PositionUpdateEvent,
        ) -> Self {
            let event = Event::new(event_key::POSITION_UPDATE)
                .add_attribute(event_key::UPDATED_AT, updated_at.to_string())
                .add_attribute(
                    event_key::DEPOSIT_COLLATERAL_DELTA,
                    deposit_collateral_delta.to_string(),
                )
                .add_attribute(
                    event_key::DEPOSIT_COLLATERAL_DELTA_USD,
                    deposit_collateral_delta_usd.to_string(),
                )
                .add_attribute(
                    event_key::ACTIVE_COLLATERAL_DELTA,
                    active_collateral_delta.to_string(),
                )
                .add_attribute(
                    event_key::ACTIVE_COLLATERAL_DELTA_USD,
                    active_collateral_delta_usd.to_string(),
                )
                .add_attribute(
                    event_key::COUNTER_COLLATERAL_DELTA,
                    counter_collateral_delta.to_string(),
                )
                .add_attribute(
                    event_key::COUNTER_COLLATERAL_DELTA_USD,
                    counter_collateral_delta_usd.to_string(),
                )
                .add_attribute(event_key::LEVERAGE_DELTA, leverage_delta.to_string())
                .add_attribute(
                    event_key::COUNTER_LEVERAGE_DELTA,
                    counter_leverage_delta.to_string(),
                )
                .add_attribute(
                    event_key::NOTIONAL_SIZE_DELTA,
                    notional_size_delta.to_string(),
                )
                .add_attribute(
                    event_key::NOTIONAL_SIZE_DELTA_USD,
                    notional_size_delta_usd.to_string(),
                )
                .add_attribute(
                    event_key::NOTIONAL_SIZE_ABS_DELTA,
                    notional_size_abs_delta.to_string(),
                )
                .add_attribute(
                    event_key::NOTIONAL_SIZE_ABS_DELTA_USD,
                    notional_size_abs_delta_usd.to_string(),
                )
                .add_attribute(event_key::TRADING_FEE_DELTA, trading_fee_delta.to_string())
                .add_attribute(
                    event_key::TRADING_FEE_DELTA_USD,
                    trading_fee_delta_usd.to_string(),
                )
                .add_attribute(
                    event_key::DELTA_NEUTRALITY_FEE_DELTA,
                    delta_neutrality_fee_delta.to_string(),
                )
                .add_attribute(
                    event_key::DELTA_NEUTRALITY_FEE_DELTA_USD,
                    delta_neutrality_fee_delta_usd.to_string(),
                );

            position_attributes.add_to_event(&event)
        }
    }

    impl TryFrom<Event> for PositionUpdateEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(Self {
                updated_at: evt.timestamp_attr(event_key::UPDATED_AT)?,
                deposit_collateral_delta: evt.number_attr(event_key::DEPOSIT_COLLATERAL_DELTA)?,
                deposit_collateral_delta_usd: evt
                    .number_attr(event_key::DEPOSIT_COLLATERAL_DELTA_USD)?,
                active_collateral_delta: evt.number_attr(event_key::ACTIVE_COLLATERAL_DELTA)?,
                active_collateral_delta_usd: evt
                    .number_attr(event_key::ACTIVE_COLLATERAL_DELTA_USD)?,
                counter_collateral_delta: evt.number_attr(event_key::COUNTER_COLLATERAL_DELTA)?,
                counter_collateral_delta_usd: evt
                    .number_attr(event_key::COUNTER_COLLATERAL_DELTA_USD)?,
                leverage_delta: evt.number_attr(event_key::LEVERAGE_DELTA)?,
                counter_leverage_delta: evt.number_attr(event_key::COUNTER_LEVERAGE_DELTA)?,
                notional_size_delta: evt.number_attr(event_key::NOTIONAL_SIZE_DELTA)?,
                notional_size_delta_usd: evt.number_attr(event_key::NOTIONAL_SIZE_DELTA_USD)?,
                notional_size_abs_delta: evt.number_attr(event_key::NOTIONAL_SIZE_ABS_DELTA)?,
                notional_size_abs_delta_usd: evt
                    .number_attr(event_key::NOTIONAL_SIZE_ABS_DELTA_USD)?,
                trading_fee_delta: evt.decimal_attr(event_key::TRADING_FEE_DELTA)?,
                trading_fee_delta_usd: evt.decimal_attr(event_key::TRADING_FEE_DELTA_USD)?,
                delta_neutrality_fee_delta: evt
                    .number_attr(event_key::DELTA_NEUTRALITY_FEE_DELTA)?,
                delta_neutrality_fee_delta_usd: evt
                    .number_attr(event_key::DELTA_NEUTRALITY_FEE_DELTA_USD)?,
                position_attributes: evt.try_into()?,
            })
        }
    }

    /// Emitted each time a position is saved
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct PositionSaveEvent {
        /// ID of the position
        pub id: PositionId,
        /// Reason the position was saved
        pub reason: PositionSaveReason,
        /// Was the position put on the pending queue?
        ///
        /// This occurs when the crank has fallen behind
        pub used_pending_queue: bool,
    }

    /// Why was a position saved?
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum PositionSaveReason {
        /// Newly opened position via market order
        OpenMarket,
        /// Update to an existing position
        Update,
        /// The crank processed this position for liquifunding
        Crank,
        /// A limit order was executed
        ExecuteLimitOrder,
        /// User attempted to set a trigger price on an existing position
        SetTrigger,
    }

    impl PositionSaveReason {
        /// Get the [CongestionReason] for this value.
        ///
        /// If this user action can result in a congestion error message,
        /// provide the [CongestionReason] value. If [None], then this
        /// [PositionSaveReason] cannot be blocked because of congestion.
        pub fn into_congestion_reason(self) -> Option<CongestionReason> {
            match self {
                PositionSaveReason::OpenMarket => Some(CongestionReason::OpenMarket),
                PositionSaveReason::Update => Some(CongestionReason::Update),
                PositionSaveReason::Crank => None,
                PositionSaveReason::ExecuteLimitOrder => None,
                PositionSaveReason::SetTrigger => Some(CongestionReason::SetTrigger),
            }
        }

        /// Represent as a string
        pub fn as_str(self) -> &'static str {
            match self {
                PositionSaveReason::OpenMarket => "open",
                PositionSaveReason::Update => "update",
                PositionSaveReason::Crank => "crank",
                PositionSaveReason::ExecuteLimitOrder => "limit-order",
                PositionSaveReason::SetTrigger => "set-trigger",
            }
        }
    }

    impl From<PositionSaveEvent> for Event {
        fn from(
            PositionSaveEvent {
                id,
                reason,
                used_pending_queue,
            }: PositionSaveEvent,
        ) -> Self {
            Event::new("position-save")
                .add_attribute("id", id.0)
                .add_attribute("reason", reason.as_str())
                .add_attribute(
                    "used-pending-queue",
                    if used_pending_queue { "true" } else { "false" },
                )
        }
    }
}
