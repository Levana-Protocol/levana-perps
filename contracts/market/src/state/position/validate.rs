use crate::prelude::*;
use msg::contracts::market::entry::SlippageAssert;

impl State<'_> {
    pub(crate) fn position_validate_trader_leverage(
        &self,
        market_type: MarketType,
        new_leverage_notional: SignedLeverageToNotional,
        current_leverage: Option<SignedLeverageToNotional>,
    ) -> Result<()> {
        let max_allowed_leverage = self
            .config
            .max_leverage
            .try_into_positive_value()
            .context("Max allowed leverage is negative")?;
        let current_leverage =
            current_leverage.map(|x| x.into_base(market_type).split().1.into_decimal256());
        let new_leverage = new_leverage_notional
            .into_base(market_type)
            .split()
            .1
            .into_decimal256();

        let is_out_of_range = if new_leverage_notional.into_number().approx_eq(Number::ZERO) {
            true
        } else {
            match current_leverage {
                Some(current_leverage) if current_leverage >= new_leverage => {
                    // We're reducing the total leverage or keeping it the same,
                    // so allow this to happen even if the new value is out of
                    // range still
                    false
                }
                _ => new_leverage > max_allowed_leverage,
            }
        };

        if is_out_of_range {
            Err(MarketError::TraderLeverageOutOfRange {
                low_allowed: Decimal256::zero(),
                high_allowed: max_allowed_leverage,
                new_leverage,
                current_leverage,
            }
            .into())
        } else {
            Ok(())
        }
    }

    fn position_validate_counter_leverage(
        &self,
        counter_leverage_to_notional: SignedLeverageToNotional,
        current_leverage: Option<SignedLeverageToNotional>,
    ) -> Result<()> {
        let max_allowed_leverage = self.config.max_leverage;

        // Get the absolute value of the new and old leverage, since validation works on those.
        let counter_leverage = counter_leverage_to_notional.into_number().abs();
        let current_leverage = current_leverage.map(|x| x.into_number().abs());

        let is_out_of_range = if !counter_leverage.approx_gt_relaxed(Number::ONE) {
            // We allow the counter leverage to be between 0 and 1 if we were already less than 1 and we're not making it any worse
            match current_leverage {
                // We're updating. If the leverage got closer to 0 then we're out of bounds
                Some(current_leverage) => counter_leverage < current_leverage,
                None => true,
            }
        } else {
            match current_leverage {
                Some(current_leverage) if current_leverage > counter_leverage => {
                    // We're reducing the total leverage, so allow this to
                    // happen even if the new value is out of range still
                    false
                }
                _ => !counter_leverage.approx_lt_relaxed(max_allowed_leverage),
            }
        };

        if is_out_of_range {
            Err(MarketError::CounterLeverageOutOfRange {
                low_allowed: Decimal256::one(),
                high_allowed: max_allowed_leverage.abs_unsigned(),
                new_leverage: counter_leverage.abs_unsigned(),
                current_leverage: current_leverage.map(|x| x.abs_unsigned()),
            }
            .into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn position_validate_leverage_data(
        &self,
        market_type: MarketType,
        new_position: &Position,
        price_point: &PricePoint,
        current_position: Option<&Position>,
    ) -> Result<()> {
        if let Some(current_position) = current_position {
            let get_direction = |pos: &Position| pos.direction().into_base(market_type);
            anyhow::ensure!(
                get_direction(current_position) == get_direction(new_position),
                "Direction changed on position update"
            );
        }

        self.position_validate_trader_leverage(
            market_type,
            new_position.active_leverage_to_notional(price_point),
            current_position.map(|p| p.active_leverage_to_notional(price_point)),
        )?;
        self.position_validate_counter_leverage(
            new_position.counter_leverage_to_notional(price_point),
            current_position.map(|p| p.counter_leverage_to_notional(price_point)),
        )?;

        Ok(())
    }

    /// Ensure we meet the requirements for minimum deposit collateral
    pub(crate) fn validate_minimum_deposit_collateral(
        &self,
        store: &dyn Storage,
        deposit_collateral: Collateral,
    ) -> Result<()> {
        let price_point = self.spot_price(store, None)?;
        let deposit = price_point.collateral_to_usd(deposit_collateral);

        // We allow up to a 10% dip on the minimum deposit to allow for price fluctuations.
        let real_minimum_ratio: Decimal256 = "0.9".parse().unwrap();

        if deposit
            < self
                .config
                .minimum_deposit_usd
                .checked_mul_dec(real_minimum_ratio)?
        {
            Err(MarketError::MinimumDeposit {
                deposit_collateral,
                deposit_usd: deposit,
                minimum_usd: self.config.minimum_deposit_usd,
            }
            .into())
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate_order_price(
        &self,
        order_price: Price,
        order_price_base: PriceBaseInQuote,
        lower_bound: Option<Price>,
        lower_bound_base: Option<PriceBaseInQuote>,
        upper_bound: Option<Price>,
        upper_bound_base: Option<PriceBaseInQuote>,
        market_type: MarketType,
        name: &str,
    ) -> Result<()> {
        let (lower_bound_comparison, upper_bound_comparison) = match market_type {
            MarketType::CollateralIsQuote => ("greater", "less"),
            MarketType::CollateralIsBase => ("less", "greater"),
        };

        if let Some(lower_bound) = lower_bound {
            anyhow::ensure!(
                order_price > lower_bound,
                "{} trigger {} must be {} than {}",
                name,
                order_price_base,
                lower_bound_comparison,
                lower_bound_base.ok_or_else(|| anyhow!("no external lower bound provided"))?
            )
        }

        if let Some(upper_bound) = upper_bound {
            anyhow::ensure!(
                order_price < upper_bound,
                "{} trigger {} must be {} than {}",
                name,
                order_price_base,
                upper_bound_comparison,
                upper_bound_base.ok_or_else(|| anyhow!("no external upper bound provided"))?
            )
        }

        Ok(())
    }

    pub(crate) fn position_validate_trigger_orders(
        &self,
        pos: &Position,
        market_type: MarketType,
        current_price: PricePoint,
    ) -> Result<()> {
        if let Some(stop_loss_override) = pos.stop_loss_override {
            match pos.direction() {
                DirectionToNotional::Long => {
                    self.validate_order_price(
                        stop_loss_override.into_notional_price(market_type),
                        stop_loss_override,
                        pos.liquidation_price,
                        pos.liquidation_price
                            .map(|price| price.into_base_price(market_type)),
                        Some(current_price.price_notional),
                        Some(current_price.price_base),
                        market_type,
                        "Stop loss",
                    )?;
                }
                DirectionToNotional::Short => {
                    self.validate_order_price(
                        stop_loss_override.into_notional_price(market_type),
                        stop_loss_override,
                        Some(current_price.price_notional),
                        Some(current_price.price_base),
                        pos.liquidation_price,
                        pos.liquidation_price
                            .map(|price| price.into_base_price(market_type)),
                        market_type,
                        "Stop loss",
                    )?;
                }
            }
        }

        if let Some(take_profit_override) = pos.take_profit_override {
            match pos.direction() {
                DirectionToNotional::Long => self.validate_order_price(
                    take_profit_override.into_notional_price(market_type),
                    take_profit_override,
                    Some(current_price.price_notional),
                    Some(current_price.price_base),
                    pos.take_profit_price,
                    pos.take_profit_price
                        .map(|price| price.into_base_price(market_type)),
                    market_type,
                    "Take profit",
                )?,
                DirectionToNotional::Short => self.validate_order_price(
                    take_profit_override.into_notional_price(market_type),
                    take_profit_override,
                    pos.take_profit_price,
                    pos.take_profit_price
                        .map(|price| price.into_base_price(market_type)),
                    Some(current_price.price_notional),
                    Some(current_price.price_base),
                    market_type,
                    "Take profit",
                )?,
            }
        }

        Ok(())
    }

    pub fn do_slippage_assert(
        &self,
        store: &dyn Storage,
        slippage_assert: SlippageAssert,
        delta_notional_size: Signed<Notional>,
        market_type: MarketType,
        delta_neutrality_fee_margin: Option<Collateral>,
    ) -> Result<()> {
        if delta_notional_size.is_zero() {
            return Ok(());
        }

        let price_point = self.spot_price(store, None)?;
        let delta_neutrality_fee = self.calc_delta_neutrality_fee(
            store,
            delta_notional_size,
            price_point,
            delta_neutrality_fee_margin,
        )?;
        let fee_rate = delta_neutrality_fee.into_number() / delta_notional_size.into_number();
        let price = price_point.price_notional.into_number() * (Number::ONE + fee_rate);

        let slippage_assert_price = slippage_assert
            .price
            .into_notional_price(market_type)
            .into_number();

        let slippage_opt = if delta_notional_size.is_strictly_positive() {
            if price <= slippage_assert_price * (Number::ONE + slippage_assert.tolerance) {
                None
            } else {
                Some(
                    (Number::from(100u64) * (price - slippage_assert_price))
                        .checked_div(slippage_assert_price),
                )
            }
        } else if price >= slippage_assert_price * (Number::ONE - slippage_assert.tolerance) {
            None
        } else {
            Some(
                (Number::from(100u64) * (slippage_assert_price - price))
                    .checked_div(slippage_assert_price),
            )
        };

        match slippage_opt {
            None => Ok(()),
            Some(slippage) => {

                Err(perp_anyhow!(
                    ErrorId::SlippageAssert,
                    ErrorDomain::Market,
                    "Slippage is exceeding provided tolerance. Slippage is {}%, max tolerance is {}%. Current price: {}. Asserted price: {}.",
                    slippage.map_or("Inf".to_string(), |s| format!("{:?}", s)),
                    Number::from(100u64) * slippage_assert.tolerance,
                    price_point.price_base,
                    slippage_assert.price,
                ))
            }
        }
    }
}

struct Currently<T>(Option<T>);
impl<T: Display> Display for Currently<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.0 {
            Some(x) => write!(f, " (currently {x})"),
            None => Ok(()),
        }
    }
}
