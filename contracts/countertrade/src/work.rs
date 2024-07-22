use cosmwasm_std::{SubMsg, WasmMsg};
use msg::contracts::market::{
    deferred_execution::GetDeferredExecResp,
    entry::{ClosedPositionCursor, ClosedPositionsResp, StatusResp},
    position::PositionQueryResponse,
};
use shared::storage::{
    DirectionToBase, DirectionToNotional, LeverageToBase, MarketType, PricePoint,
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

    desired_action(state, &status, pos.as_deref()).map(|x| match x {
        Some(desc) => HasWorkResp::Work { desc },
        None => HasWorkResp::NoWork {},
    })
}

fn desired_action(
    state: &State,
    status: &StatusResp,
    pos: Option<&PositionQueryResponse>,
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
        // FIXME do actual calculations

        // if status.long_funding > state.config.max_funding.into_signed() {
        //     Ok(HasWorkResp::Work {
        //         desc: WorkDescription::OpenPosition {
        //             direction: DirectionToBase::Short,
        //             leverage: max_leverage,
        //             collateral,
        //             take_profit: shared::storage::TakeProfitTrader::Finite(
        //                 NonZero::new(
        //                     price
        //                         .price_base
        //                         .into_non_zero()
        //                         .raw()
        //                         .checked_mul(Decimal256::from_ratio(9u32, 10u32))?,
        //                 )
        //                 .context("Impossible 0 from multiplying take profit price")?,
        //             ),
        //         },
        //     })
        // } else if status.short_funding > state.config.max_funding.into_signed() {
        //     Ok(HasWorkResp::Work {
        //         desc: WorkDescription::OpenPosition {
        //             direction: DirectionToBase::Long,
        //             leverage: max_leverage,
        //             collateral,
        //             take_profit: shared::storage::TakeProfitTrader::Finite(
        //                 NonZero::new(
        //                     price
        //                         .price_base
        //                         .into_non_zero()
        //                         .raw()
        //                         .checked_mul(Decimal256::from_ratio(11u32, 10u32))?,
        //                 )
        //                 .context("Impossible 0 from multiplying take profit price")?,
        //             ),
        //         },
        //     })
        // } else {
        //     Ok(HasWorkResp::NoWork {})
        // }
        Ok(Some(WorkDescription::OpenPosition {
            direction: todo!(),
            leverage: todo!(),
            collateral: todo!(),
            take_profit: todo!(),
        }))
    }
}

pub(crate) fn execute(
    storage: &mut dyn Storage,
    state: State,
    market: MarketInfo,
    sender: Addr,
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
            let keys = crate::state::SHARES.keys(storage, None, None, Order::Ascending);
            let mut to_remove = vec![];
            for key in keys {
                let (addr, market_id) = key?;
                if market_id == market.id {
                    to_remove.push(addr);
                }
            }
            for item in to_remove {
                crate::state::SHARES.remove(storage, (&item, &market.id));
            }
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
