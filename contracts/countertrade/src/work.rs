use std::str::FromStr;

use cosmwasm_std::{SubMsg, WasmMsg};
use perpswap::contracts::market::{
    deferred_execution::GetDeferredExecResp,
    entry::{ClosedPositionCursor, ClosedPositionsResp, StatusResp},
    position::{PositionId, PositionQueryResponse},
};
use perpswap::{
    number::Number,
    price::{Price, TakeProfitTrader},
    storage::{
        DirectionToBase, DirectionToNotional, LeverageToBase, MarketType, PricePoint,
        SignedLeverageToNotional,
    },
};

use crate::prelude::*;

pub(crate) fn get_work_for(
    _storage: &dyn Storage,
    state: &State,
    market: &MarketInfo,
    totals: &Totals,
) -> Result<HasWorkResp> {
    // Optimization: no shares, so there's no possibility of work to do
    if totals.shares.is_zero() {
        return Ok(HasWorkResp::NoWork {});
    }

    // Check if we finished executing a deferred exec item
    if let Some(id) = totals.deferred_exec {
        match state.querier.query_wasm_smart::<GetDeferredExecResp>(
            &market.addr,
            &MarketQueryMsg::GetDeferredExec { id },
        )? {
            GetDeferredExecResp::Found { item } => match item.status {
                perpswap::contracts::market::deferred_execution::DeferredExecStatus::Pending => {
                    return Ok(HasWorkResp::NoWork {});
                }
                perpswap::contracts::market::deferred_execution::DeferredExecStatus::Success {
                    ..
                }
                | perpswap::contracts::market::deferred_execution::DeferredExecStatus::Failure {
                    ..
                } => {
                    return Ok(HasWorkResp::Work {
                        desc: WorkDescription::ClearDeferredExec { id },
                    })
                }
            },
            GetDeferredExecResp::NotFound {} => bail!(
                "For market {}, cannot find expected deferred exec item {id}",
                market.id
            ),
        }
    }

    // Check for newly closed positions to update collateral
    let ClosedPositionsResp {
        mut positions,
        // We ignore the cursor here and generated our own.
        // This cursor will be None if there are no more closed positions.
        // However, we want to always have a value to catch future closed positions.
        cursor: _,
    } = state.querier.query_wasm_smart(
        &market.addr,
        &MarketQueryMsg::ClosedPositionHistory {
            owner: state.my_addr.as_ref().into(),
            cursor: totals.last_closed.clone().map(|mut cursor| {
                // This is probably a misdesign in the cursor API in the market contract.
                // All other bounds in cw-storage-plus are exclusive. However, this one
                // is inclusive. So adapt to it by using the next position ID.
                cursor.position = cursor.position.next();
                cursor
            }),
            limit: Some(1),
            order: Some(perpswap::storage::OrderInMessage::Ascending),
        },
    )?;
    assert!(positions.len() <= 1);
    if let Some(closed) = positions.pop() {
        return Ok(HasWorkResp::Work {
            desc: WorkDescription::CollectClosedPosition {
                pos_id: closed.id,
                close_time: closed.close_time,
                active_collateral: market
                    .token
                    .round_down_to_precision(closed.active_collateral)?,
            },
        });
    }

    let pos = PositionsInfo::load(state, market)?;

    let pos = match pos {
        PositionsInfo::TooManyPositions { to_close } => {
            return Ok(HasWorkResp::Work {
                desc: WorkDescription::ClosePosition { pos_id: to_close },
            })
        }
        PositionsInfo::NoPositions => None,
        PositionsInfo::OnePosition { pos } => Some(pos),
    };

    if totals.collateral.is_zero() && pos.is_none() {
        return Ok(HasWorkResp::Work {
            desc: WorkDescription::ResetShares,
        });
    }

    // If we have zero collateral available, we have no work to
    // perform.
    if totals.collateral.is_zero() && pos.is_some() {
        return Ok(HasWorkResp::NoWork {});
    }

    let price: PricePoint = state
        .querier
        .query_wasm_smart(&market.addr, &MarketQueryMsg::SpotPrice { timestamp: None })
        .context("Unable to query market spot price")?;
    let status: StatusResp = state
        .querier
        .query_wasm_smart(&market.addr, &MarketQueryMsg::Status { price: None })
        .context("Unable to query market status")?;

    let (long_interest, short_interest) = match status.market_type {
        MarketType::CollateralIsQuote => (status.long_notional, status.short_notional),
        MarketType::CollateralIsBase => (status.short_notional, status.long_notional),
    };

    let available_collateral = NonZero::new(totals.collateral)
        .context("Impossible, zero collateral after checking that we have a minimum deposit")?;
    let minimum_position_collateral = price.usd_to_collateral(status.config.minimum_deposit_usd);

    // We try to close popular-side positions.
    if let Some(pos) = &pos {
        let funding = match pos.direction_to_base {
            DirectionToBase::Long => status.long_funding,
            DirectionToBase::Short => status.short_funding,
        };

        if funding.is_positive_or_zero() {
            let notional_diff = match pos.direction_to_base {
                DirectionToBase::Long => status.long_notional.checked_sub(status.short_notional)?,
                DirectionToBase::Short => {
                    status.short_notional.checked_sub(status.long_notional)?
                }
            };

            if pos.notional_size.abs_unsigned() < notional_diff
                || pos.deposit_collateral <= minimum_position_collateral.into_signed()
            {
                // This means that closing this position won't make
                // the countertrade direction unpopular. So we have to
                // close it, to make countertrade contract open it in
                // opposite direction eventually.
                return Ok(HasWorkResp::Work {
                    desc: WorkDescription::ClosePosition { pos_id: pos.id },
                });
            } else {
                // Update the existing position to target_funding rate
                let result = determine_target_notional(
                    long_interest,
                    short_interest,
                    state.config.target_funding.into_number(),
                    &status,
                    state.config.iterations,
                    Some(*pos.clone()),
                )?;
                match result {
                    Some(position_notional_size) => {
                        let max_leverage = state.config.max_leverage;
                        let take_profit_factor = state.config.take_profit_factor;
                        let stop_loss_factor = state.config.stop_loss_factor;
                        let result = compute_delta_notional(
                            position_notional_size,
                            &price,
                            &status,
                            available_collateral,
                            max_leverage,
                            take_profit_factor,
                            stop_loss_factor,
                            Some(*pos.clone()),
                            market,
                            state,
                        )?;
                        match result {
                            Some(work) => return Ok(HasWorkResp::Work { desc: work }),
                            None => return Ok(HasWorkResp::NoWork {}),
                        }
                    }
                    None => {
                        return Ok(HasWorkResp::NoWork {});
                    }
                }
            }
        }
    }

    let collateral_in_usd = price.collateral_to_usd(totals.collateral);
    if collateral_in_usd < status.config.minimum_deposit_usd {
        return Ok(HasWorkResp::NoWork {});
    }

    desired_action(
        state,
        &status,
        &price,
        pos.as_deref(),
        available_collateral,
        market,
    )
    .map(|x| match x {
        Some(desc) => HasWorkResp::Work { desc },
        None => HasWorkResp::NoWork {},
    })
}

fn desired_action(
    state: &State,
    status: &StatusResp,
    price: &PricePoint,
    pos: Option<&PositionQueryResponse>,
    available_collateral: NonZero<Collateral>,
    market_info: &MarketInfo,
) -> Result<Option<WorkDescription>> {
    let one_sided_market = if status.long_funding.is_zero() || status.short_funding.is_zero() {
        assert!(status.long_funding.is_zero());
        assert!(status.short_funding.is_zero());
        // Handle the case where we have a one sided market
        match (
            status.long_notional.is_zero(),
            status.short_notional.is_zero(),
        ) {
            // No positions opened at all, everything is fine
            (true, true) => return Ok(None),
            // Perfectly balanced positions are opened
            (false, false) => {
                assert!(status.long_notional == status.short_notional);
                return Ok(None);
            }
            // In these cases, we have a one sided market
            (true, false) | (false, true) => true,
        }
    } else {
        false
    };

    // Now entering the flipped zone: code below here will deal exclusively with internal direction/prices/etc.
    let (long_funding, short_funding) = match status.market_type {
        MarketType::CollateralIsQuote => (status.long_funding, status.short_funding),
        MarketType::CollateralIsBase => (status.short_funding, status.long_funding),
    };

    let (long_interest, short_interest) = match status.market_type {
        MarketType::CollateralIsQuote => (status.long_notional, status.short_notional),
        MarketType::CollateralIsBase => (status.short_notional, status.long_notional),
    };

    let min_funding = state.config.min_funding.into_signed();
    let max_funding = state.config.max_funding.into_signed();
    let target_funding = state.config.target_funding.into_signed();

    let (popular_funding, unpopular_interest) = if long_funding.is_strictly_positive() {
        assert!(short_funding.is_negative() || one_sided_market);
        (long_funding, short_interest)
    } else {
        assert!(long_funding.is_negative() || one_sided_market);
        (short_funding, long_interest)
    };

    if popular_funding >= min_funding && popular_funding <= max_funding {
        assert!(popular_funding.is_zero() || !one_sided_market);
        Ok(None)
    } else if popular_funding < min_funding {
        match pos {
            Some(pos) => {
                // If we have the only unpopular position, keep it open
                if pos.notional_size.abs_unsigned() == unpopular_interest {
                    Ok(None)
                } else {
                    Ok(Some(WorkDescription::ClosePosition { pos_id: pos.id }))
                }
            }
            None => {
                if one_sided_market {
                    let allowed_iterations = state.config.iterations;
                    // Returns the target notional size of a newly constructed position
                    let result = determine_target_notional(
                        long_interest,
                        short_interest,
                        target_funding,
                        status,
                        allowed_iterations,
                        None,
                    )?;
                    match result {
                        Some(position_notional_size) => {
                            let max_leverage = state.config.max_leverage;
                            let take_profit_factor = state.config.take_profit_factor;
                            let stop_loss_factor = state.config.stop_loss_factor;
                            compute_delta_notional(
                                position_notional_size,
                                price,
                                status,
                                available_collateral,
                                max_leverage,
                                take_profit_factor,
                                stop_loss_factor,
                                None,
                                market_info,
                                state,
                            )
                        }

                        None => Ok(None),
                    }
                } else {
                    Ok(None)
                }
            }
        }
    } else {
        let allowed_iterations = state.config.iterations;
        match pos {
            Some(pos) => {
                let delta = pos
                    .active_collateral
                    .into_number()
                    .checked_sub(available_collateral.into_number())?
                    .abs()
                    .checked_div(available_collateral.into_number())?;
                if delta < Decimal256::from_ratio(5u32, 10u32).into_number() {
                    Ok(None)
                } else {
                    let result = determine_target_notional(
                        long_interest,
                        short_interest,
                        target_funding,
                        status,
                        allowed_iterations,
                        Some(pos.clone()),
                    )?;
                    match result {
                        Some(position_notional_size) => {
                            let max_leverage = state.config.max_leverage;
                            let take_profit_factor = state.config.take_profit_factor;
                            let stop_loss_factor = state.config.stop_loss_factor;
                            compute_delta_notional(
                                position_notional_size,
                                price,
                                status,
                                available_collateral,
                                max_leverage,
                                take_profit_factor,
                                stop_loss_factor,
                                Some(pos.clone()),
                                market_info,
                                state,
                            )
                        }
                        None => Ok(None),
                    }
                }
            }
            None => {
                // Returns the target notional size of a newly constructed position
                let result = determine_target_notional(
                    long_interest,
                    short_interest,
                    target_funding,
                    status,
                    allowed_iterations,
                    None,
                )?;

                match result {
                    Some(position_notional_size) => {
                        let max_leverage = state.config.max_leverage;
                        let take_profit_factor = state.config.take_profit_factor;
                        let stop_loss_factor = state.config.stop_loss_factor;
                        compute_delta_notional(
                            position_notional_size,
                            price,
                            status,
                            available_collateral,
                            max_leverage,
                            take_profit_factor,
                            stop_loss_factor,
                            None,
                            market_info,
                            state,
                        )
                    }
                    None => Ok(None),
                }
            }
        }
    }
}

/// Deterimine target notional on the unpopoular side
fn determine_target_notional(
    long_interest: Notional,
    short_interest: Notional,
    target_funding: Number,
    status: &StatusResp,
    allowed_iterations: u8,
    countertrade_position: Option<PositionQueryResponse>,
) -> Result<Option<Signed<Notional>>> {
    match countertrade_position {
        Some(countertrade_position) => {
            let (pos_long_interest, pos_short_interest) = {
                let direction = countertrade_position
                    .direction_to_base
                    .into_notional(status.market_type);
                match direction {
                    DirectionToNotional::Long => (
                        countertrade_position.notional_size.abs_unsigned(),
                        Notional::zero(),
                    ),
                    DirectionToNotional::Short => (
                        Notional::zero(),
                        countertrade_position.notional_size.abs_unsigned(),
                    ),
                }
            };
            let long_interest = long_interest.checked_sub(pos_long_interest)?;
            let short_interest = short_interest.checked_sub(pos_short_interest)?;
            let desired_notional = smart_search(
                long_interest,
                short_interest,
                target_funding,
                status,
                allowed_iterations,
                0,
            )?;
            let position_notional_size = if long_interest < short_interest {
                desired_notional.into_signed()
            } else {
                -desired_notional.into_signed()
            };

            Ok(Some(position_notional_size))
        }
        None => {
            let desired_notional = smart_search(
                long_interest,
                short_interest,
                target_funding,
                status,
                allowed_iterations,
                0,
            )?;
            let position_notional_size = if long_interest < short_interest {
                desired_notional.into_signed()
            } else {
                -desired_notional.into_signed()
            };
            Ok(Some(position_notional_size))
        }
    }
}

/// Returns the delta notional on the unpopular side.
fn smart_search(
    long_notional: Notional,
    short_notional: Notional,
    target_funding: Number,
    status: &StatusResp,
    allowed_iterations: u8,
    mut iteration: u8,
) -> Result<Notional> {
    let (popular_notional, unpopular_notional) = if long_notional > short_notional {
        (long_notional, short_notional)
    } else {
        (short_notional, long_notional)
    };

    // Takes care of PERP-4149
    // In case both popular and unpopular sides are 0, we need to close any CT position, or do
    // nothing
    if unpopular_notional == Notional::zero() && popular_notional == Notional::zero() {
        return Ok(Notional::zero());
    }

    // Ratio refers to what percentage of the market is unpopular.
    // The absolute maximum we can ever achieve is 0.5, meaning a
    // perfectly balanced market.
    //
    // The lowest ratio we'll potentially want is the starting ratio.
    // The fact that we're in this function means we know we want to
    // increase the unpopular side positions.

    let mut high_ratio = Decimal256::from_ratio(1u8, 2u8);
    let mut low_ratio = unpopular_notional.into_decimal256().checked_div(
        popular_notional
            .into_decimal256()
            .checked_add(unpopular_notional.into_decimal256())?,
    )?;
    loop {
        iteration += 1;
        let target_ratio = high_ratio
            .checked_add(low_ratio)?
            .checked_div("2".parse().unwrap())?;

        let desired_unpopular = Notional::from_decimal256(
            target_ratio
                .checked_mul(popular_notional.into_decimal256())?
                .checked_div(Decimal256::one().checked_sub(target_ratio)?)?,
        );

        assert!(popular_notional >= desired_unpopular);
        let new_funding_rate = derive_popular_funding_rate_annual(
            popular_notional,
            desired_unpopular,
            &status.config,
        )?;

        let difference = new_funding_rate
            .into_signed()
            .checked_sub(target_funding)?
            .abs_unsigned();
        let epsilon = Decimal256::from_str("0.00001").unwrap();
        if desired_unpopular < unpopular_notional {
            break Ok(Notional::zero());
        } else if difference < epsilon {
            let delta_unpopular = desired_unpopular.checked_sub(unpopular_notional)?;
            break Ok(delta_unpopular);
        } else if iteration >= allowed_iterations {
            break Err(anyhow!("Iteration limit reached without converging"));
        } else if new_funding_rate.into_signed() > target_funding {
            low_ratio = target_ratio;
        } else {
            high_ratio = target_ratio;
        }
    }
}

fn derive_popular_funding_rate_annual(
    popular_notional: Notional,
    unpopular_notional: Notional,
    config: &perpswap::contracts::market::config::Config,
) -> Result<Decimal256> {
    let rf_per_annual_cap = config.funding_rate_max_annualized;
    let instant_net_open_interest = popular_notional
        .into_number()
        .checked_sub(unpopular_notional.into_number())?;
    let instant_open_short = unpopular_notional;
    let instant_open_long = popular_notional;
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
    let (popular, unpopular) = if instant_open_long.is_zero() || instant_open_short.is_zero() {
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

    if instant_open_long.is_zero() || instant_open_short.is_zero() {
        assert!(unpopular.is_zero());
        assert!(popular.is_zero());
    } else {
        assert!(unpopular.is_negative());
        assert!(popular.is_strictly_positive());
    }
    Ok(popular.abs_unsigned())
}

#[allow(clippy::too_many_arguments)]
fn compute_delta_notional(
    position_notional_size: Signed<Notional>,
    price: &PricePoint,
    status: &StatusResp,
    available_collateral: NonZero<Collateral>,
    max_leverage: LeverageToBase,
    take_profit_factor: Decimal256,
    stop_loss_factor: Decimal256,
    countertrade_position: Option<PositionQueryResponse>,
    market_info: &MarketInfo,
    state: &State,
) -> Result<Option<WorkDescription>> {
    let entry_price = price.price_notional;
    let hundred = Number::from_str("100").context("Unable to convert 100 to Number")?;
    let take_profit_factor_diff = take_profit_factor
        .into_number()
        .checked_mul(entry_price.into_number())?
        .checked_div(hundred)?;
    let take_profit = if position_notional_size.is_strictly_positive() {
        entry_price
            .into_number()
            .checked_add(take_profit_factor_diff)?
    } else {
        entry_price
            .into_number()
            .checked_sub(take_profit_factor_diff)?
    };
    let take_profit = Price::try_from_number(take_profit)?;
    let take_profit = TakeProfitTrader::from(take_profit.into_base_price(status.market_type));

    let stop_loss_factor_diff = stop_loss_factor
        .into_number()
        .checked_mul(entry_price.into_number())?
        .checked_div(hundred)?;
    let stop_loss = if position_notional_size.is_strictly_positive() {
        entry_price
            .into_number()
            .checked_sub(stop_loss_factor_diff)?
    } else {
        entry_price
            .into_number()
            .checked_add(stop_loss_factor_diff)?
    };

    let stop_loss_override =
        Some(Price::try_from_number(stop_loss)?.into_base_price(status.market_type));

    let market_max_leverage = status.config.max_leverage;
    let market_max_leverage = LeverageToBase::from(
        NonZero::new(
            market_max_leverage
                .try_into_non_negative_value()
                .context("Market max_leverage is negative")?,
        )
        .context("Market max_leverage is zero")?,
    );
    let desired_leverage = max_leverage.min(market_max_leverage);

    let notional_direction = if position_notional_size.is_strictly_positive() {
        DirectionToNotional::Long
    } else {
        DirectionToNotional::Short
    };
    let base_direction = notional_direction.into_base(status.market_type);
    let desired_leverage = desired_leverage.into_signed(base_direction);
    let desired_leverage = desired_leverage.into_notional(status.market_type)?;

    let position_notional_size_in_collateral =
        position_notional_size.map(|size| price.notional_to_collateral(size));

    let min_deposit_collateral = price.usd_to_collateral(status.config.minimum_deposit_usd);

    let capital = optimize_capital_efficiency(
        available_collateral,
        position_notional_size_in_collateral,
        desired_leverage,
        countertrade_position.clone(),
        status.market_type,
        min_deposit_collateral,
        market_info,
        price,
        state,
    )?;

    let work = match capital {
        Some(capital) => match capital {
            Capital::New {
                deposit_collateral,
                leverage,
            } => {
                let deposit_collateral = NonZero::new(deposit_collateral)
                    .context("deposit_collateral is zero")?
                    .min(available_collateral);

                if deposit_collateral.into_decimal256() < min_deposit_collateral.into_decimal256() {
                    // Market is skewed, and deserves to be balanced, but the size of the skew
                    // is tiny. Wait for the market to have more interest before intervening.
                    return Ok(None);
                }

                let (direction, leverage) = leverage.into_base(status.market_type)?.split();
                WorkDescription::OpenPosition {
                    direction,
                    leverage,
                    collateral: deposit_collateral,
                    take_profit,
                    stop_loss_override,
                }
            }
            Capital::AddCollateral { collateral, pos_id } => {
                // Make sure we're providing more collateral than the crank fee.
                // As an arbitrary metric, we make sure we always have at least 5x the crank fee,
                // otherwise we'll just bleed funds into fees.
                let crank_fee_collateral = estimate_crank_fee_from_status(status, price)?
                    .checked_mul_dec(Decimal256::from_ratio(5u8, 1u8))?;
                if crank_fee_collateral >= collateral {
                    return Ok(None);
                }
                WorkDescription::UpdatePositionAddCollateralImpactSize {
                    pos_id,
                    amount: NonZero::new(collateral).context("add_collateral is zero")?,
                }
            }
            Capital::RemoveCollateral {
                collateral,
                pos_id,
                crank_fee,
            } => WorkDescription::UpdatePositionRemoveCollateralImpactSize {
                pos_id,
                amount: NonZero::new(collateral).context("remove_collateral is zero")?,
                crank_fee,
            },
            Capital::Close { pos_id } => WorkDescription::ClosePosition { pos_id },
        },
        None => return Ok(None),
    };
    Ok(Some(work))
}

enum Capital {
    New {
        deposit_collateral: Collateral,
        leverage: SignedLeverageToNotional,
    },
    AddCollateral {
        collateral: Collateral,
        pos_id: PositionId,
    },
    RemoveCollateral {
        collateral: Collateral,
        pos_id: PositionId,
        crank_fee: Collateral,
    },
    Close {
        pos_id: PositionId,
    },
}

/// Returns the deposit collateral and leverage value to be used for this position.
#[allow(clippy::too_many_arguments)]
fn optimize_capital_efficiency(
    available_collateral: NonZero<Collateral>,
    position_notional_size_in_collateral: Signed<Collateral>,
    desired_leverage: SignedLeverageToNotional,
    countertrade_position: Option<PositionQueryResponse>,
    market_type: MarketType,
    min_deposit_collateral: Collateral,
    market_info: &MarketInfo,
    price: &PricePoint,
    state: &State,
) -> Result<Option<Capital>> {
    let result = match countertrade_position {
        Some(countertrade_position) => {
            let ct_position_leverage = countertrade_position
                .leverage
                .into_signed(countertrade_position.direction_to_base)
                .into_notional(market_type)?;
            let deposit_collateral = position_notional_size_in_collateral
                .into_number()
                .checked_div(ct_position_leverage.into_number())?;
            let deposit_collateral = Collateral::from_decimal256(deposit_collateral.abs_unsigned());

            let diff = deposit_collateral
                .into_signed()
                .checked_sub(countertrade_position.deposit_collateral)?;
            if diff.is_strictly_positive() {
                // We should add more collateral
                let collateral = diff.abs_unsigned();
                let collateral = if collateral > available_collateral.raw() {
                    available_collateral.raw()
                } else {
                    collateral
                };
                Some(Capital::AddCollateral {
                    collateral,
                    pos_id: countertrade_position.id,
                })
            } else if diff.is_zero() {
                None
            } else {
                // We should reduce collateral
                let estimated_crank_fee = estimate_crank_fee(state, market_info, price)?;
                let collateral = diff.abs_unsigned();
                let countertrade_final_active_collateral = countertrade_position
                    .active_collateral
                    .into_signed()
                    .checked_sub(collateral.into_signed())?;
                let max_deduct = if countertrade_final_active_collateral
                    >= min_deposit_collateral.into_signed()
                {
                    collateral
                } else {
                    let result = countertrade_position
                        .deposit_collateral
                        .checked_sub(min_deposit_collateral.into_signed())?;
                    result.abs_unsigned()
                };
                if estimated_crank_fee > max_deduct {
                    // If crank_fee is more than the amount it's going
                    // to be reduce, it's not worth performing this
                    // action
                    None
                } else if max_deduct >= countertrade_position.active_collateral.raw() {
                    Some(Capital::Close {
                        pos_id: countertrade_position.id,
                    })
                } else {
                    Some(Capital::RemoveCollateral {
                        collateral: max_deduct,
                        pos_id: countertrade_position.id,
                        crank_fee: estimated_crank_fee,
                    })
                }
            }
        }
        None => {
            let deposit_collateral = position_notional_size_in_collateral
                .into_number()
                .checked_div(desired_leverage.into_number())?;
            assert!(deposit_collateral.is_strictly_positive());
            let deposit_collateral = Collateral::from_decimal256(deposit_collateral.abs_unsigned());

            let deposit_collateral = if deposit_collateral > available_collateral.raw() {
                available_collateral.raw()
            } else {
                deposit_collateral
            };
            Some(Capital::New {
                deposit_collateral,
                leverage: desired_leverage,
            })
        }
    };

    Ok(result)
}

pub(crate) fn execute(
    storage: &mut dyn Storage,
    state: State,
    market: MarketInfo,
) -> Result<Response> {
    let mut totals = crate::state::TOTALS
        .may_load(storage, &market.id)?
        .unwrap_or_default();

    let work = get_work_for(storage, &state, &market, &totals)?;

    let desc = match work {
        HasWorkResp::NoWork {} => bail!("No work items available"),
        HasWorkResp::Work { desc } => desc,
    };

    let mut res = Response::new()
        .add_event(Event::new("work-desc").add_attribute("desc", format!("{desc:?}")));

    let add_market_msg =
        |storage: &mut dyn Storage, res: Response, msg: WasmMsg| -> Result<Response> {
            assert!(!crate::state::REPLY_MARKET.exists(storage));
            crate::state::REPLY_MARKET.save(storage, &market.id)?;
            Ok(res.add_submessage(SubMsg::reply_on_success(msg, 0)))
        };

    match desc {
        WorkDescription::OpenPosition {
            direction,
            leverage,
            collateral,
            take_profit,
            stop_loss_override,
        } => {
            let event = Event::new("open-position")
                .add_attribute("direction", direction.as_str())
                .add_attribute("leverage", leverage.to_string())
                .add_attribute("collateral", collateral.to_string())
                .add_attribute("take_profit", take_profit.to_string())
                .add_attribute("market", market.id.as_str());
            let event = if let Some(stop_loss_override) = stop_loss_override {
                event.add_attribute("stop_loss_override", stop_loss_override.to_string())
            } else {
                event
            };
            res = res.add_event(event);
            let msg = market.token.into_market_execute_msg(
                &market.addr,
                collateral.raw(),
                MarketExecuteMsg::OpenPosition {
                    slippage_assert: None,
                    leverage,
                    direction,
                    stop_loss_override,
                    take_profit,
                },
            )?;
            totals.collateral = totals.collateral.checked_sub(collateral.raw())?;
            crate::state::TOTALS.save(storage, &market.id, &totals)?;

            res = add_market_msg(storage, res, msg)?;
        }
        WorkDescription::ClosePosition { pos_id } => {
            res = res.add_event(
                Event::new("close-position")
                    .add_attribute("position-id", pos_id.to_string())
                    .add_attribute("market", market.id.as_str()),
            );
            let msg = cosmwasm_std::WasmMsg::Execute {
                contract_addr: market.addr.into_string(),
                msg: to_json_binary(&MarketExecuteMsg::ClosePosition {
                    id: pos_id,
                    slippage_assert: None,
                })?,
                funds: vec![],
            };
            res = add_market_msg(storage, res, msg)?;
        }
        WorkDescription::CollectClosedPosition {
            pos_id,
            close_time,
            active_collateral,
        } => {
            totals.last_closed = Some(ClosedPositionCursor {
                time: close_time,
                position: pos_id,
            });
            totals.collateral = totals.collateral.checked_add(active_collateral)?;
            crate::state::TOTALS.save(storage, &market.id, &totals)?;
            res = res.add_event(
                Event::new("collect-closed-position")
                    .add_attribute("position-id", pos_id.to_string())
                    .add_attribute("active-collateral", active_collateral.to_string())
                    .add_attribute("market", market.id.as_str()),
            );
        }
        WorkDescription::ResetShares => {
            let remove_keys: Vec<_> = crate::state::REVERSE_SHARES
                .prefix(&market.id)
                .range(storage, None, None, Order::Ascending)
                .collect();

            for key in remove_keys {
                let (addr, _) = key?;
                crate::state::SHARES.remove(storage, (&addr, &market.id));
                crate::state::REVERSE_SHARES.remove(storage, (&market.id, &addr));
            }
            crate::state::TOTALS.remove(storage, &market.id);

            res = res
                .add_event(Event::new("reset-shares").add_attribute("market", market.id.as_str()));
        }
        WorkDescription::ClearDeferredExec { id } => {
            assert_eq!(totals.deferred_exec, Some(id));
            totals.deferred_exec = None;
            crate::state::TOTALS.save(storage, &market.id, &totals)?;
            res = res.add_event(
                Event::new("clear-deferred-exec")
                    .add_attribute("deferred-exec-id", id.to_string())
                    .add_attribute("market", market.id.as_str()),
            )
        }
        WorkDescription::UpdatePositionAddCollateralImpactSize { pos_id, amount } => {
            let event = Event::new("update-position-add-collateral-impact-size")
                .add_attribute("position-id", pos_id.to_string())
                .add_attribute("amount", amount.to_string());
            res = res.add_event(event);

            let msg = market.token.into_market_execute_msg(
                &market.addr,
                amount.raw(),
                MarketExecuteMsg::UpdatePositionAddCollateralImpactSize {
                    id: pos_id,
                    slippage_assert: None,
                },
            )?;
            totals.collateral = totals.collateral.checked_sub(amount.raw())?;
            crate::state::TOTALS.save(storage, &market.id, &totals)?;

            res = add_market_msg(storage, res, msg)?;
        }
        WorkDescription::UpdatePositionRemoveCollateralImpactSize {
            pos_id,
            amount,
            crank_fee,
        } => {
            let event = Event::new("update-position-remove-collateral-impact-size")
                .add_attribute("position-id", pos_id.to_string())
                .add_attribute("crank-fee", crank_fee.to_string())
                .add_attribute("amount", amount.to_string());
            res = res.add_event(event);

            let amount = market.token.round_down_to_precision(amount.raw())?;
            let amount = NonZero::new(amount).context("Remove amount is zero")?;

            let market_msg = MarketExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                id: pos_id,
                amount,
                slippage_assert: None,
            };
            let msg = market
                .token
                .into_market_execute_msg(&market.addr, crank_fee, market_msg)?;

            totals.collateral = totals.collateral.checked_add(amount.raw())?;
            totals.collateral = totals.collateral.checked_sub(crank_fee)?;
            crate::state::TOTALS.save(storage, &market.id, &totals)?;
            res = add_market_msg(storage, res, msg)?;
        }
    }

    Ok(res)
}

fn estimate_crank_fee(
    state: &State,
    market: &MarketInfo,
    price: &PricePoint,
) -> Result<Collateral> {
    // Loginc taken from from deferred_execution part of the code.
    let status: perpswap::contracts::market::entry::StatusResp = state
        .querier
        .query_wasm_smart(
            &market.addr,
            &perpswap::contracts::market::entry::QueryMsg::Status { price: None },
        )
        .with_context(|| format!("Unable to load market status from contract {}", market.addr))?;
    estimate_crank_fee_from_status(&status, price)
}

fn estimate_crank_fee_from_status(status: &StatusResp, price: &PricePoint) -> Result<Collateral> {
    let crank_fee_surcharge = status.config.crank_fee_surcharge;
    let crank_fee_charged = status.config.crank_fee_charged;
    let estimated_queue_size = 5u32;
    let fees =
        crank_fee_surcharge.checked_mul_dec(Decimal256::from_ratio(estimated_queue_size, 10u32))?;
    let fees = fees.checked_add(crank_fee_charged)?;
    let fees = price.usd_to_collateral(fees);
    Ok(fees)
}
