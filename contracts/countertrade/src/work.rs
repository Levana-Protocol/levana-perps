use std::str::FromStr;

use cosmwasm_std::{SubMsg, WasmMsg};
use msg::contracts::market::{
    deferred_execution::GetDeferredExecResp,
    entry::{ClosedPositionCursor, ClosedPositionsResp, StatusResp},
    position::PositionQueryResponse,
};
use shared::{
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
                msg::contracts::market::deferred_execution::DeferredExecStatus::Pending => {
                    return Ok(HasWorkResp::NoWork {})
                }
                msg::contracts::market::deferred_execution::DeferredExecStatus::Success {
                    ..
                }
                | msg::contracts::market::deferred_execution::DeferredExecStatus::Failure {
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
            order: Some(shared::storage::OrderInMessage::Ascending),
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

    let price: PricePoint = state
        .querier
        .query_wasm_smart(&market.addr, &MarketQueryMsg::SpotPrice { timestamp: None })
        .context("Unable to query market spot price")?;
    let status: StatusResp = state
        .querier
        .query_wasm_smart(&market.addr, &MarketQueryMsg::Status { price: None })
        .context("Unable to query market status")?;

    // We always close popular-side positions. Future potential optimization:
    // reduce position size instead when possible.
    if let Some(pos) = &pos {
        let funding = match pos.direction_to_base {
            DirectionToBase::Long => status.long_funding,
            DirectionToBase::Short => status.short_funding,
        };
        // We close on 0 also
        if funding.is_positive_or_zero() {
            return Ok(HasWorkResp::Work {
                desc: WorkDescription::ClosePosition { pos_id: pos.id },
            });
        }
    }

    let collateral_in_usd = price.collateral_to_usd(totals.collateral);
    if collateral_in_usd < status.config.minimum_deposit_usd {
        return Ok(HasWorkResp::NoWork {});
    }

    let collateral = NonZero::new(totals.collateral)
        .context("Impossible, zero collateral after checking that we have a minimum deposit")?;

    let max_leverage = state.config.max_leverage.min(LeverageToBase::from(
        NonZero::new(status.config.max_leverage.abs_unsigned())
            .context("Invalid 0 max_leverage in market")?,
    ));

    desired_action(state, &status, &price, pos.as_deref(), collateral).map(|x| match x {
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
) -> Result<Option<WorkDescription>> {
    if status.long_funding.is_zero() || status.short_funding.is_zero() {
        assert!(status.long_funding.is_zero());
        assert!(status.short_funding.is_zero());
        return Ok(None);
    }

    // Now entering the flipped zone: code below here will deal exclusively with internal direction/prices/etc.
    let (long_funding, short_funding) = match status.market_type {
        MarketType::CollateralIsQuote => (status.long_funding, status.short_funding),
        MarketType::CollateralIsBase => (status.short_funding, status.long_funding),
    };

    let (long_interest, short_interest) = match status.market_type {
        MarketType::CollateralIsQuote => (status.long_notional, status.short_notional),
        MarketType::CollateralIsBase => (status.short_notional, status.long_notional),
    };

    let current_direction = pos.map(|pos| pos.direction_to_base.into_notional(status.market_type));
    let min_funding = state.config.min_funding.into_signed();
    let max_funding = state.config.max_funding.into_signed();
    let target_funding = state.config.target_funding.into_signed();

    let (popular_funding, unpop_funding, popular_direction) = if long_funding.is_strictly_positive()
    {
        assert!(short_funding.is_negative());
        (long_funding, short_funding, DirectionToNotional::Long)
    } else {
        assert!(long_funding.is_negative());
        (short_funding, long_funding, DirectionToNotional::Short)
    };

    if popular_funding >= min_funding && popular_funding <= max_funding {
        Ok(None)
    } else if popular_funding < min_funding {
        match pos {
            Some(pos) => Ok(Some(WorkDescription::ClosePosition { pos_id: pos.id })),
            None => Ok(None),
        }
    } else {
        match pos {
            Some(pos) => {
                // The idea here is that we will close the existing
                // countertrade position and open a new one later in
                // the first version of countertrade contract.  But a
                // better way of doing this is to update the existing
                // position in future iteration of this contract.
                Ok(Some(WorkDescription::ClosePosition { pos_id: pos.id }))
            }
            None => {
                // Returns the target notional size of a newly constructed position
                let result = determine_target_notional(
                    long_interest,
                    short_interest,
                    min_funding,
                    target_funding,
                    status,
                )?;

                println!("Determined target_notional");

                match result {
                    TargetNotionalResult::NoWork => Ok(None),
                    TargetNotionalResult::Result {
                        direction,
                        desired_notional,
                    } => {
                        let position_notional_size = match direction {
                            DirectionToNotional::Long => desired_notional.into_signed(),
                            DirectionToNotional::Short => -desired_notional.into_signed(),
                        };
                        compute_delta_notional(
                            position_notional_size,
                            price,
                            status,
                            available_collateral,
                        )
                    }
                }
            }
        }
    }
}

enum TargetNotionalResult {
    NoWork,
    Result {
        direction: DirectionToNotional,
        desired_notional: Notional,
    },
}

fn determine_target_notional(
    long_interest: Notional,
    short_interest: Notional,
    min_funding: Number,
    target_funding: Number,
    status: &StatusResp,
) -> Result<TargetNotionalResult> {
    println!("Going to derive instant funding rate_annual");
    let (rfl, rfs) = derive_instant_funding_rate_annual(long_interest, short_interest, status)?;
    let total_open_interest = long_interest.checked_add(short_interest)?;
    struct TempResult {
        unpopular_side: DirectionToNotional,
        starting_ratio: Number,
        unpopular_rf: Signed<Decimal256>,
    }
    let result = if long_interest < short_interest {
        TempResult {
            unpopular_side: DirectionToNotional::Long,
            starting_ratio: long_interest
                .into_number()
                .checked_div(total_open_interest.into_number())?,
            unpopular_rf: rfl,
        }
    } else {
        TempResult {
            unpopular_side: DirectionToNotional::Short,
            starting_ratio: short_interest
                .into_number()
                .checked_div(total_open_interest.into_number())?,
            unpopular_rf: rfs,
        }
    };

    if result.unpopular_rf > min_funding {
        return Ok(TargetNotionalResult::NoWork);
    }

    if result.starting_ratio > Number::from_str("1.4")? {
        bail!("Starting_ratio should not be greater than 1.4")
    }

    println!("Going to smart search");
    let open_interest = smart_search(
        long_interest,
        short_interest,
        result.unpopular_side,
        target_funding,
        result.starting_ratio,
        status,
        0,
    )?;
    match result.unpopular_side {
        DirectionToNotional::Long => {
            let desired_notional = open_interest.long.checked_sub(long_interest)?;
            Ok(TargetNotionalResult::Result {
                direction: result.unpopular_side,
                desired_notional,
            })
        }
        DirectionToNotional::Short => {
            let desired_notional = open_interest.short.checked_sub(short_interest)?;
            Ok(TargetNotionalResult::Result {
                direction: result.unpopular_side,
                desired_notional,
            })
        }
    }
}

struct OpenInterest {
    long: Notional,
    short: Notional,
}

#[allow(clippy::too_many_arguments)]
fn smart_search(
    long_notional: Notional,
    short_notional: Notional,
    unpopular_side: DirectionToNotional,
    target_funding: Number,
    starting_ratio: Number,
    status: &StatusResp,
    mut iteration: u8,
) -> Result<OpenInterest> {
    let mut high_ratio = Number::from_str("0.5").unwrap();
    let mut low_ratio = starting_ratio;
    loop {
        iteration += 1;
        let target_ratio = high_ratio
            .checked_add(low_ratio)?
            .checked_div("2".parse().unwrap())?;
        println!("Iteration: {iteration}. long_notional: {long_notional}. short_notional: {short_notional}. unpopular_side: {unpopular_side:?}. target_funding: {target_funding}. starting_ratio: {starting_ratio}. high_ratio: {high_ratio}. low_ratio: {low_ratio}. target_ratio: {target_ratio}. market type: {:?}. status.long_funding: {}. status.short_funding: {}. status.long_notional: {}. status.short_notional: {}", status.market_type,status.long_funding,status.short_funding,status.long_notional,status.short_notional);
        let total_open_interest = long_notional.checked_add(short_notional)?;

        let open_interest = match unpopular_side {
            DirectionToNotional::Long => {
                let long = total_open_interest
                    .into_number()
                    .checked_mul(target_ratio)?
                    .checked_sub(long_notional.into_number())?
                    .checked_div(target_ratio)?;
                let long = long_notional.into_number().checked_add(long)?;
                let long = Notional::try_from_number(long)?;
                OpenInterest {
                    long,
                    short: short_notional,
                }
            }
            DirectionToNotional::Short => {
                let short = total_open_interest
                    .into_number()
                    .checked_mul(target_ratio)?
                    .checked_sub(short_notional.into_number())?
                    .checked_div(target_ratio)?;
                let short = short_notional.into_number().checked_add(short)?;
                let short = Notional::try_from_number(short)?;
                OpenInterest {
                    long: long_notional,
                    short,
                }
            }
        };

        let (new_rfl, new_rfs) =
            derive_instant_funding_rate_annual(open_interest.long, open_interest.short, status)?;

        let new_funding_rate = match unpopular_side {
            DirectionToNotional::Long => new_rfs,
            DirectionToNotional::Short => new_rfl,
        };

        println!("new_funding_rate: {new_funding_rate}, new_rfl {new_rfl} & new_rfs {new_rfs}");
        println!("target_funding: {}", target_funding);

        let difference = new_funding_rate.checked_sub(target_funding)?.abs_unsigned();
        let epsilon = Decimal256::from_str("0.00001").unwrap();
        if difference < epsilon {
            break Ok(open_interest);
        } else if iteration >= 50 {
            break Err(anyhow!("Iteration limit reached without converging"));
        } else if new_funding_rate > target_funding {
            low_ratio = target_ratio;
        } else {
            high_ratio = target_ratio;
        }
    }
}

fn derive_instant_funding_rate_annual(
    long_notional: Notional,
    short_notional: Notional,
    status: &StatusResp,
) -> Result<(Number, Number)> {
    let config = &status.config;
    let rf_per_annual_cap = config.funding_rate_max_annualized;
    let instant_net_open_interest = long_notional
        .into_number()
        .checked_sub(short_notional.into_number())?;
    let instant_open_short = short_notional;
    let instant_open_long = long_notional;
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
    let (long_rate, short_rate) = if instant_open_long.is_zero() || instant_open_short.is_zero() {
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

fn compute_delta_notional(
    position_notional_size: Signed<Notional>,
    price: &PricePoint,
    status: &StatusResp,
    available_collateral: NonZero<Collateral>,
) -> Result<Option<WorkDescription>> {
    println!("market_id: {}", status.market_id);
    println!("market_type: {:?}", status.market_type);

    let entry_price = price.price_notional;
    let factor = Number::from_str("1.5")
        .context("Unable to convert 1.5 to Decimal256")?
        .into_number();
    let take_profit = if position_notional_size.is_strictly_positive() {
        Price::try_from_number(entry_price.into_number().checked_mul(factor)?)?
    } else {
        let factor_diff = factor
            .checked_div(Number::from_str("100").context("Unable to convert 100 to Number")?)?;
        let factor_diff = factor_diff.checked_mul(entry_price.into_number())?;
        Price::try_from_number(entry_price.into_number().checked_sub(factor_diff)?)?
    };
    let take_profit = TakeProfitTrader::from(take_profit.into_base_price(status.market_type));

    let desired_leverage = Number::from_str("10")
        .context("Unable to convert 10 to Number")?
        .min(status.config.max_leverage);

    let desired_leverage =
        SignedLeverageToNotional::from(if position_notional_size.is_strictly_positive() {
            desired_leverage
        } else {
            -desired_leverage
        });

    let position_notional_size_in_collateral =
        position_notional_size.map(|size| price.notional_to_collateral(size));

    let (deposit_collateral, leverage) =
        optimize_capital_efficiency(position_notional_size_in_collateral, desired_leverage)?;

    let min_deposit_collateral = price.usd_to_collateral(status.config.minimum_deposit_usd);
    if deposit_collateral < min_deposit_collateral {
        // Market is skewed, and deserves to be balanced, but the size of the skew
        // is tiny. Wait for the market to have more interest before intervening.
        return Ok(None);
    }

    let deposit_collateral = NonZero::new(deposit_collateral)
        .context("collateral is zero")?
        .min(available_collateral);

    let (direction, leverage) = leverage.into_base(status.market_type)?.split();

    println!("counter trade contract recommendation:");
    println!("collateral: {deposit_collateral}");
    println!("leverage: {leverage}");
    println!("take_profit: {take_profit}");
    println!("entry_price: {entry_price}");
    println!("direction: {:?}", direction);

    Ok(Some(WorkDescription::OpenPosition {
        direction,
        leverage,
        collateral: deposit_collateral,
        take_profit,
    }))
}

/// Returns the deposit collateral and leverage value to be used for this position.
fn optimize_capital_efficiency(
    position_notional_size_in_collateral: Signed<Collateral>,
    desired_leverage: SignedLeverageToNotional,
) -> Result<(Collateral, SignedLeverageToNotional)> {
    let deposit_collateral = position_notional_size_in_collateral
        .into_number()
        .checked_div(desired_leverage.into_number())?;
    assert!(deposit_collateral.is_strictly_positive());
    let deposit_collateral = Collateral::from_decimal256(deposit_collateral.abs_unsigned());

    Ok((deposit_collateral, desired_leverage))
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
        } => {
            res = res.add_event(
                Event::new("open-position")
                    .add_attribute("direction", direction.as_str())
                    .add_attribute("leverage", leverage.to_string())
                    .add_attribute("collateral", collateral.to_string())
                    .add_attribute("take_profit", take_profit.to_string())
                    .add_attribute("market", market.id.as_str()),
            );
            let msg = market.token.into_market_execute_msg(
                &market.addr,
                collateral.raw(),
                MarketExecuteMsg::OpenPosition {
                    slippage_assert: None,
                    leverage,
                    direction,
                    max_gains: None,
                    stop_loss_override: None,
                    take_profit: Some(take_profit),
                },
            )?;
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
    }

    Ok(res)
}
