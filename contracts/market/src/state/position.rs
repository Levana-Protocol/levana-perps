pub(crate) mod liquifund;

use cosmwasm_std::Order;
mod open;
use msg::contracts::market::{
    entry::{ClosedPositionCursor, ClosedPositionsResp, PositionsQueryFeeApproach},
    position::events::{PositionSaveEvent, PositionSaveReason},
};
pub(crate) use open::*;
mod close;
pub use close::*;
pub(crate) mod update;
mod validate;
pub use validate::*;
mod cw721;
pub use cw721::*;

use crate::constants::DEFAULT_CLOSED_POSITION_HISTORY_LIMIT;
use crate::prelude::*;
use cw_storage_plus::PrefixBound;
use msg::contracts::market::position::{ClosedPosition, LiquidationReason, PositionOrPendingClose};

pub(super) const OPEN_POSITIONS: Map<PositionId, Position> = Map::new(namespace::OPEN_POSITIONS);
pub(super) const LAST_POSITION_ID: Item<PositionId> = Item::new(namespace::LAST_POSITION_ID);

// running totals of notional
pub(super) const OPEN_NOTIONAL_LONG_INTEREST: Item<Notional> =
    Item::new(namespace::OPEN_NOTIONAL_LONG_INTEREST);
pub(super) const OPEN_NOTIONAL_SHORT_INTEREST: Item<Notional> =
    Item::new(namespace::OPEN_NOTIONAL_SHORT_INTEREST);

pub struct LiquidatablePosition {
    pub id: PositionId,
    pub reason: LiquidationReason,
}
// liquidation price tracking

/// Maps a price trigger to a position id for descending prices. Uses a composite key to effectively create a multimap.
///
/// This is used for long liquidations and short max gains.
pub(super) const PRICE_TRIGGER_DESC: Map<(PriceKey, PositionId), LiquidationReason> =
    Map::new(namespace::PRICE_TRIGGER_DESC);
/// Maps a price trigger to a position id for ascending prices. Uses a composite key to effectively create a multimap.
///
/// This is used for short liquidations and long max gains.
pub(super) const PRICE_TRIGGER_ASC: Map<(PriceKey, PositionId), LiquidationReason> =
    Map::new(namespace::PRICE_TRIGGER_ASC);

// history
pub(super) const CLOSED_POSITION_HISTORY: Map<(&Addr, (Timestamp, PositionId)), ClosedPosition> =
    Map::new(namespace::CLOSED_POSITION_HISTORY);

/// Direct lookup of closed positions by ID
const CLOSED_POSITIONS: Map<PositionId, ClosedPosition> = Map::new(namespace::CLOSED_POSITIONS);

/// When is the next time we should try to run the liquifunding process for this position?
///
/// Invariant: we must have an entry here for every open position. There must be
/// no entries here for non-open positions. No timestamp can be more than
/// [Config::liquifunding_delay_seconds] in the future.
pub(super) const NEXT_LIQUIFUNDING: Map<(Timestamp, PositionId), ()> =
    Map::new(namespace::NEXT_LIQUIFUNDING);

/// Gets a full position by id
pub(crate) fn get_position(store: &dyn Storage, id: PositionId) -> Result<Position> {
    #[derive(serde::Serialize)]
    struct Data {
        position: PositionId,
    }
    OPEN_POSITIONS
        .may_load(store, id)
        .map_err(|e| anyhow!("Could not parse position {id}: {e:?}"))?
        .ok_or_else(|| MarketError::MissingPosition { id: id.to_string() }.into_anyhow())
}

fn already(
    dir: DirectionToBase,
    cap: Number,
    sensitivity: Number,
    instant_before: Number,
    net_notional_before: Signed<Notional>,
    net_notional_after: Signed<Notional>,
) -> MarketError {
    match dir {
        DirectionToBase::Long => MarketError::DeltaNeutralityFeeAlreadyLong {
            cap,
            sensitivity,
            instant_before,
            net_notional_before,
            net_notional_after,
        },
        DirectionToBase::Short => MarketError::DeltaNeutralityFeeAlreadyShort {
            cap,
            sensitivity,
            instant_before,
            net_notional_before,
            net_notional_after,
        },
    }
}

fn newly(
    dir: DirectionToBase,
    cap: Number,
    sensitivity: Number,
    instant_after: Number,
    net_notional_before: Signed<Notional>,
    net_notional_after: Signed<Notional>,
) -> MarketError {
    match dir {
        DirectionToBase::Long => MarketError::DeltaNeutralityFeeNewlyLong {
            cap,
            sensitivity,
            instant_after,
            net_notional_before,
            net_notional_after,
        },
        DirectionToBase::Short => MarketError::DeltaNeutralityFeeNewlyShort {
            cap,
            sensitivity,
            instant_after,
            net_notional_before,
            net_notional_after,
        },
    }
}

fn flipped(
    dir: DirectionToBase,
    cap: Number,
    sensitivity: Number,
    instant_before: Number,
    instant_after: Number,
    net_notional_before: Signed<Notional>,
    net_notional_after: Signed<Notional>,
) -> MarketError {
    match dir {
        DirectionToBase::Long => MarketError::DeltaNeutralityFeeShortToLong {
            cap,
            sensitivity,
            instant_before,
            instant_after,
            net_notional_before,
            net_notional_after,
        },
        DirectionToBase::Short => MarketError::DeltaNeutralityFeeLongToShort {
            cap,
            sensitivity,
            instant_before,
            instant_after,
            net_notional_before,
            net_notional_after,
        },
    }
}

impl State<'_> {
    // Retrieve a page / slice of closed positions
    // default order is descending
    pub(crate) fn closed_positions_history(
        &self,
        store: &dyn Storage,
        owner: Addr,
        cursor: Option<ClosedPositionCursor>,
        order: Option<OrderInMessage>,
        limit: Option<u32>,
    ) -> Result<ClosedPositionsResp> {
        // keep it as utils order so we can compare (native Order doesn't impl Eq)
        let order = order.unwrap_or(OrderInMessage::Descending);
        let limit: usize = limit
            .unwrap_or(DEFAULT_CLOSED_POSITION_HISTORY_LIMIT)
            .try_into()?;

        let (min, max) = match (cursor, order) {
            (None, _) => (None, None),
            (Some(cursor), OrderInMessage::Ascending) => {
                (Some(Bound::inclusive((cursor.time, cursor.position))), None)
            }
            (Some(cursor), OrderInMessage::Descending) => {
                (None, Some(Bound::inclusive((cursor.time, cursor.position))))
            }
        };

        let mut iter = CLOSED_POSITION_HISTORY
            .prefix(&owner)
            .range(store, min, max, order.into());

        let mut positions: Vec<ClosedPosition> = Vec::new();

        let continuation_cursor = loop {
            match iter.next() {
                // got to the end, nothing more to do and no cursor to continue from
                None => {
                    break None;
                }
                Some(res) => {
                    let (key, pos) = res?;
                    // continuations only exist when we reach a limit and break early
                    if positions.len() == limit {
                        // slight optimization, to avoid needless pagination
                        if iter.next().is_some() {
                            break Some(ClosedPositionCursor {
                                time: key.0,
                                position: key.1,
                            });
                        } else {
                            break None;
                        }
                    }
                    positions.push(pos);
                }
            }
        };

        Ok(ClosedPositionsResp {
            positions,
            cursor: continuation_cursor,
        })
    }

    /// Validate that we can perform the net open interest adjustment described
    pub(crate) fn check_adjust_net_open_interest(
        &self,
        store: &dyn Storage,
        notional_size_diff: Signed<Notional>,
        dir: DirectionToNotional,
        assert_delta_neutrality_fee_cap: bool,
    ) -> Result<AdjustOpenInterestResult> {
        let long_before = self.open_long_interest(store)?;
        let short_before = self.open_short_interest(store)?;

        let long_after;
        let short_after;
        let adjust_res;
        match dir {
            DirectionToNotional::Long => {
                long_after = long_before
                    .checked_add_signed(notional_size_diff)
                    .context("adjust_net_open_interest: long interest would be negative")?;
                short_after = short_before;
                adjust_res = AdjustOpenInterestResult::Long(long_after);
            }
            DirectionToNotional::Short => {
                long_after = long_before;
                short_after = short_before
                    .checked_add_signed(-notional_size_diff)
                    .context("adjust_net_open_interest: short interest would be negative")?;
                adjust_res = AdjustOpenInterestResult::Short(short_after);
            }
        };

        if assert_delta_neutrality_fee_cap {
            let net_notional_before = long_before
                .into_signed()
                .checked_sub(short_before.into_signed())?;
            let net_notional_after = long_after
                .into_signed()
                .checked_sub(short_after.into_signed())?;

            let cap: Number = self.config.delta_neutrality_fee_cap.into();
            let sensitivity: Number = self.config.delta_neutrality_fee_sensitivity.into();

            let is_capped_low = |x| x <= -cap;
            let is_capped_high = |x| x >= cap;

            let instant_delta_neutrality_before_uncapped =
                net_notional_before.into_number() / sensitivity;
            let instant_delta_neutrality_after_uncapped =
                net_notional_after.into_number() / sensitivity;

            let is_capped_low_before = is_capped_low(instant_delta_neutrality_before_uncapped);
            let is_capped_high_before = is_capped_high(instant_delta_neutrality_before_uncapped);
            let is_capped_low_after = is_capped_low(instant_delta_neutrality_after_uncapped);
            let is_capped_high_after = is_capped_high(instant_delta_neutrality_after_uncapped);

            // these strings are just to make error messages easier to understand
            // since the UX is in terms of DirectionToBase, not DirectionToNotional
            let market_type = self.market_type(store)?;
            // May be different from dir, since updating/closing a position can
            // cause a notional size diff which is opposite to the position
            // direction.
            let notional_direction = if notional_size_diff.is_positive_or_zero() {
                DirectionToNotional::Long
            } else {
                DirectionToNotional::Short
            };
            let base_direction = notional_direction.into_base(market_type);

            let res = if is_capped_low_before {
                match notional_direction {
                    // We were already too short, disallow going shorter
                    DirectionToNotional::Short => Err(already(
                        base_direction,
                        cap,
                        sensitivity,
                        instant_delta_neutrality_before_uncapped,
                        net_notional_before,
                        net_notional_after,
                    )),
                    // We don't allow the user to swing the market all the way from capped low to capped high
                    DirectionToNotional::Long => {
                        if is_capped_high_after {
                            Err(flipped(
                                base_direction,
                                cap,
                                sensitivity,
                                instant_delta_neutrality_before_uncapped,
                                instant_delta_neutrality_after_uncapped,
                                net_notional_before,
                                net_notional_after,
                            ))
                        } else {
                            Ok(())
                        }
                    }
                }
            } else if is_capped_high_before {
                match notional_direction {
                    // We were already too long, disallow going longer
                    DirectionToNotional::Long => Err(already(
                        base_direction,
                        cap,
                        sensitivity,
                        instant_delta_neutrality_before_uncapped,
                        net_notional_before,
                        net_notional_after,
                    )),
                    // We don't allow the user to swing the market all the way from capped high to capped low
                    DirectionToNotional::Short => {
                        if is_capped_low_after {
                            Err(flipped(
                                base_direction,
                                cap,
                                sensitivity,
                                instant_delta_neutrality_before_uncapped,
                                instant_delta_neutrality_after_uncapped,
                                net_notional_before,
                                net_notional_after,
                            ))
                        } else {
                            Ok(())
                        }
                    }
                }
            } else if is_capped_low_after {
                debug_assert!(notional_size_diff <= Signed::zero());
                Err(newly(
                    base_direction,
                    cap,
                    sensitivity,
                    instant_delta_neutrality_after_uncapped,
                    net_notional_before,
                    net_notional_after,
                ))
            } else if is_capped_high_after {
                debug_assert!(notional_size_diff >= Signed::zero());
                Err(newly(
                    base_direction,
                    cap,
                    sensitivity,
                    instant_delta_neutrality_after_uncapped,
                    net_notional_before,
                    net_notional_after,
                ))
            } else {
                Ok(())
            };

            res.map(|()| adjust_res).map_err(MarketError::into_anyhow)
        } else {
            Ok(adjust_res)
        }
    }

    pub(crate) fn adjust_net_open_interest(
        &self,
        ctx: &mut StateContext,
        notional_size_diff: Signed<Notional>,
        dir: DirectionToNotional,
        assert_delta_neutrality_fee_cap: bool,
    ) -> Result<()> {
        self.check_adjust_net_open_interest(
            ctx.storage,
            notional_size_diff,
            dir,
            assert_delta_neutrality_fee_cap,
        )?
        .store(ctx)?;

        Ok(())
    }

    pub(crate) fn open_long_interest(&self, store: &dyn Storage) -> Result<Notional> {
        OPEN_NOTIONAL_LONG_INTEREST
            .load(store)
            .map_err(|err| err.into())
    }

    pub(crate) fn open_short_interest(&self, store: &dyn Storage) -> Result<Notional> {
        OPEN_NOTIONAL_SHORT_INTEREST
            .load(store)
            .map_err(|err| err.into())
    }

    pub(crate) fn positions_net_open_interest(
        &self,
        store: &dyn Storage,
    ) -> Result<Signed<Notional>> {
        Ok(self.open_long_interest(store)?.into_signed()
            - self.open_short_interest(store)?.into_signed())
    }

    pub(crate) fn position_token_addr(&self, store: &dyn Storage) -> Result<Addr> {
        load_external_map(
            &self.querier,
            &self.factory_address,
            namespace::POSITION_TOKEN_ADDRS,
            self.market_id(store)?,
        )
    }

    pub(crate) fn pos_snapshot_for_open(
        &self,
        store: &dyn Storage,
        mut pos: Position,
        fees: PositionsQueryFeeApproach,
    ) -> Result<PositionOrPendingClose> {
        let config = &self.config;
        let market_type = self.market_id(store)?.get_market_type();
        let entry_price =
            self.spot_price(store, pos.price_point_created_at.unwrap_or(pos.created_at))?;
        let spot_price = self.current_spot_price(store)?;

        // PERP-996 ensure we do not flip direction, see comments in
        // liquifunding for more details
        let original_direction_to_base = pos
            .active_leverage_to_notional(&spot_price)
            .into_base(market_type)
            .split()
            .0;

        // We calculate the DNF fee that would be applied on closing the
        // position.
        let dnf_on_close_collateral = self.calc_delta_neutrality_fee(
            store,
            -pos.notional_size,
            &spot_price,
            Some(pos.liquidation_margin.delta_neutrality),
        )?;

        let (calc_pending_fees, include_dnf) = match fees {
            PositionsQueryFeeApproach::NoFees => (false, false),
            PositionsQueryFeeApproach::Accumulated => (true, false),
            PositionsQueryFeeApproach::AllFees => (true, true),
        };

        if calc_pending_fees {
            // Calculate pending fees

            // Even though the usage of self.now() looks incorrect below, this is only used for
            // querying positions, and therefore calculating till now without a liquifunding
            // or precise price point is a best estimate of fees.
            let (borrow_fees, _) =
                self.calc_capped_borrow_fee_payment(store, &pos, pos.liquifunded_at, self.now())?;
            let borrow_fees = borrow_fees.lp.checked_add(borrow_fees.xlp)?;
            let (funding_payments, _) = self.calc_capped_funding_payment(
                store,
                &pos,
                pos.liquifunded_at,
                self.now(),
                true,
            )?;
            let delta_neutrality_fee = if include_dnf {
                dnf_on_close_collateral
            } else {
                Signed::zero()
            };
            pos.borrow_fee
                .checked_add_assign(borrow_fees, &spot_price)?;
            pos.funding_fee
                .checked_add_assign(funding_payments, &spot_price)?;
            pos.delta_neutrality_fee
                .checked_add_assign(delta_neutrality_fee, &spot_price)?;

            pos.liquidation_margin.borrow = pos
                .liquidation_margin
                .borrow
                .checked_sub(borrow_fees)
                .ok()
                .unwrap_or_default();
            pos.liquidation_margin.funding = pos
                .liquidation_margin
                .funding
                .checked_add_signed(-funding_payments)
                .ok()
                .unwrap_or_default();
            pos.liquidation_margin
                .delta_neutrality
                .checked_add_signed(-delta_neutrality_fee)
                .ok()
                .unwrap_or_default();

            let active_collateral = pos
                .active_collateral
                .into_signed()
                .checked_sub(borrow_fees.into_signed())?
                .checked_sub(funding_payments)?
                .checked_sub(delta_neutrality_fee)?;
            pos.active_collateral = match active_collateral.try_into_non_zero() {
                Some(x) => x,
                // This should never happen, since it would mean we have
                // insufficient liquidation margin. But if we do end up in that case
                // in production, just use a small value, it will be picked up by
                // the extrapolate step below.
                None => {
                    debug_assert!(false, "Impossible situation encountered, active collateral would go zero or negative in query");
                    "0.00001".parse().unwrap()
                }
            };
        };

        let start_price = self.spot_price(store, pos.liquifunded_at)?;
        pos.into_query_response_extrapolate_exposure(
            start_price,
            spot_price,
            entry_price.price_notional,
            config,
            market_type,
            original_direction_to_base,
            dnf_on_close_collateral,
        )
    }

    pub(crate) fn position_assert_owner(
        &self,
        store: &dyn Storage,
        pos_id: PositionId,
        addr: &Addr,
    ) -> Result<()> {
        match get_position(store, pos_id) {
            Err(_) => Err(perp_anyhow!(
                ErrorId::Auth,
                ErrorDomain::Market,
                "position owner does not exist",
            )),
            Ok(pos) => {
                if pos.owner != *addr {
                    Err(perp_anyhow!(
                        ErrorId::Auth,
                        ErrorDomain::Market,
                        "position owner is {} not {}",
                        pos.owner,
                        addr
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    pub(crate) fn liquidatable_position(
        &self,
        store: &dyn Storage,
        price: Price,
    ) -> Result<Option<LiquidatablePosition>> {
        // let's say spot price = 10
        // and our long position liquidation prices are 10,11,12
        // then we liquidate these
        // if our long position prices were 7,8,9
        // then we would not liquidate
        if let Some(res) = PRICE_TRIGGER_DESC
            .prefix_range(
                store,
                Some(PrefixBound::inclusive(price)),
                None,
                Order::Descending,
            )
            .next()
        {
            let ((_, id), reason) = res?;
            return Ok(Some(LiquidatablePosition { id, reason }));
        }

        if let Some(res) = PRICE_TRIGGER_ASC
            .prefix_range(
                store,
                None,
                Some(PrefixBound::inclusive(price)),
                Order::Ascending,
            )
            .next()
        {
            let ((_, id), reason) = res?;
            return Ok(Some(LiquidatablePosition { id, reason }));
        }

        Ok(None)
    }

    /// Would the given new price cause a position to be liquidatable?
    pub(crate) fn newly_liquidatable_position(
        &self,
        store: &dyn Storage,
        oracle_price: Price,
        new_price: Price,
    ) -> bool {
        PRICE_TRIGGER_DESC
            .prefix_range(
                store,
                Some(PrefixBound::inclusive(new_price)),
                Some(PrefixBound::exclusive(oracle_price)),
                Order::Descending,
            )
            .next()
            .is_some()
            || PRICE_TRIGGER_ASC
                .prefix_range(
                    store,
                    Some(PrefixBound::exclusive(oracle_price)),
                    Some(PrefixBound::inclusive(new_price)),
                    Order::Ascending,
                )
                .next()
                .is_some()
    }
}

pub(crate) fn positions_init(store: &mut dyn Storage) -> Result<()> {
    LAST_POSITION_ID.save(store, &PositionId::new(0))?;
    OPEN_NOTIONAL_SHORT_INTEREST.save(store, &Notional::zero())?;
    OPEN_NOTIONAL_LONG_INTEREST.save(store, &Notional::zero())?;
    Ok(())
}

impl State<'_> {
    /// Remove old entries for a position. This should provide the [Position]
    /// value loaded directly from storage without modifications.
    fn position_remove(&self, ctx: &mut StateContext, pos_id: PositionId) -> Result<()> {
        // Load up the original position, since we need the exact price points
        // stored there for managing other data structures like the liquidation
        // prices.
        let position = get_position(ctx.storage, pos_id)?;

        debug_assert!(OPEN_POSITIONS.has(ctx.storage, position.id));
        debug_assert!(NEXT_LIQUIFUNDING.has(ctx.storage, (position.next_liquifunding, position.id)));

        OPEN_POSITIONS.remove(ctx.storage, position.id);
        NEXT_LIQUIFUNDING.remove(ctx.storage, (position.next_liquifunding, position.id));

        self.remove_liquidation_prices(ctx, &position)?;
        self.decrease_total_funding_margin(ctx, position.liquidation_margin.funding)?;

        Ok(())
    }

    /// A version of [State::position_save] that does not recalculate any values.
    ///
    /// This is intended for when a code path needs to make minor modifications
    /// to the [Position] value and then store it again.
    pub(crate) fn position_save_no_recalc(
        &self,
        ctx: &mut StateContext,
        position: &Position,
    ) -> Result<()> {
        debug_assert!(OPEN_POSITIONS.has(ctx.storage, position.id));
        OPEN_POSITIONS
            .save(ctx.storage, position.id, position)
            .map_err(|e| e.into())
    }

    /// Save an open position into the [OPEN_POSITIONS] data structure.
    ///
    /// This function will recalculate a number of fields on the [Position]
    /// value, such as liquidation margin, liquidation prices, etc.
    pub(crate) fn position_save(
        &self,
        ctx: &mut StateContext,
        pos: &mut Position,
        price_point: &PricePoint,
        is_update: bool,
        recalc_liquidation_margin: bool,
        reason: PositionSaveReason,
    ) -> Result<()> {
        if is_update {
            self.position_remove(ctx, pos.id)?;
        } else {
            debug_assert!(!OPEN_POSITIONS.has(ctx.storage, pos.id));
        }

        if recalc_liquidation_margin {
            debug_assert_eq!(price_point.timestamp, pos.liquifunded_at);
            pos.liquidation_margin = pos.liquidation_margin(price_point, &self.config)?;
        } else {
            debug_assert_eq!(
                pos.liquidation_margin,
                pos.liquidation_margin(price_point, &self.config)?
            );
        }

        perp_ensure!(
            pos.active_collateral.raw() >= pos.liquidation_margin.total(),
            ErrorId::InsufficientMargin,
            ErrorDomain::Market,
            "Active collateral cannot be less than liquidation margin: {} vs {:?}",
            pos.active_collateral,
            pos.liquidation_margin
        );

        pos.liquidation_price = pos.liquidation_price(
            price_point.price_notional,
            pos.active_collateral,
            &pos.liquidation_margin,
        );
        let market_type = self.market_type(ctx.storage)?;
        pos.take_profit_price = pos.take_profit_price(price_point, market_type)?;

        debug_assert!(pos.liquifunded_at < pos.next_liquifunding);

        OPEN_POSITIONS.save(ctx.storage, pos.id, pos)?;
        NEXT_LIQUIFUNDING.save(ctx.storage, (pos.next_liquifunding, pos.id), &())?;
        self.store_liquidation_prices(ctx, pos)?;

        self.increase_total_funding_margin(ctx, pos.liquidation_margin.funding)?;

        ctx.response
            .add_event(PositionSaveEvent { id: pos.id, reason });

        Ok(())
    }

    /// Removes a position's liquidation price and take profit prices
    fn remove_liquidation_prices(&self, ctx: &mut StateContext, pos: &Position) -> Result<()> {
        match pos.direction() {
            DirectionToNotional::Long => {
                if let Some(liquidation_price) = pos.liquidation_price {
                    PRICE_TRIGGER_DESC.remove(ctx.storage, (liquidation_price.into(), pos.id));
                }

                if let Some(take_profit_price) = pos.take_profit_price {
                    PRICE_TRIGGER_ASC.remove(ctx.storage, (take_profit_price.into(), pos.id));
                }

                if let Some(stop_loss_override) = pos.stop_loss_override_notional {
                    PRICE_TRIGGER_DESC.remove(ctx.storage, (stop_loss_override.into(), pos.id));
                }

                if let Some(take_profit_override) = pos.take_profit_override_notional {
                    PRICE_TRIGGER_ASC.remove(ctx.storage, (take_profit_override.into(), pos.id));
                }
            }
            DirectionToNotional::Short => {
                if let Some(liquidation_price) = pos.liquidation_price {
                    PRICE_TRIGGER_ASC.remove(ctx.storage, (liquidation_price.into(), pos.id));
                }

                if let Some(take_profit_price) = pos.take_profit_price {
                    PRICE_TRIGGER_DESC.remove(ctx.storage, (take_profit_price.into(), pos.id));
                }

                if let Some(stop_loss_override) = pos.stop_loss_override_notional {
                    PRICE_TRIGGER_ASC.remove(ctx.storage, (stop_loss_override.into(), pos.id));
                }

                if let Some(take_profit_override) = pos.take_profit_override_notional {
                    PRICE_TRIGGER_DESC.remove(ctx.storage, (take_profit_override.into(), pos.id));
                }
            }
        }

        Ok(())
    }

    /// Only used for migration, make sure this no-longer-needed data structure is empty.
    pub(crate) fn ensure_liquidation_prices_pending_empty(
        &self,
        store: &dyn Storage,
    ) -> Result<()> {
        const LIQUIDATION_PRICES_PENDING: Map<(Timestamp, PositionId), ()> =
            Map::new(namespace::LIQUIDATION_PRICES_PENDING);
        const LIQUIDATION_PRICES_PENDING_REVERSE: Map<PositionId, Timestamp> =
            Map::new(namespace::LIQUIDATION_PRICES_PENDING_REVERSE);
        const LIQUIDATION_PRICES_PENDING_COUNT: Item<u32> =
            Item::new(namespace::LIQUIDATION_PRICES_PENDING_COUNT);

        anyhow::ensure!(LIQUIDATION_PRICES_PENDING
            .keys(store, None, None, Order::Ascending)
            .next()
            .is_none());
        anyhow::ensure!(LIQUIDATION_PRICES_PENDING_REVERSE
            .keys(store, None, None, Order::Ascending)
            .next()
            .is_none());
        anyhow::ensure!(
            LIQUIDATION_PRICES_PENDING_COUNT
                .may_load(store)?
                .unwrap_or_default()
                == 0
        );

        Ok(())
    }

    /// Actually store the liquidation prices
    ///
    /// This can either happen because we tried to store new prices and the
    /// protocol's crank was up to date, _or_ because the protocol was lagging
    /// behind on the crank and we're now unpending a queued liquidation price.
    fn store_liquidation_prices(&self, ctx: &mut StateContext, pos: &Position) -> Result<()> {
        match pos.direction() {
            DirectionToNotional::Long => {
                if let Some(liquidation_price) = pos.liquidation_price {
                    PRICE_TRIGGER_DESC.save(
                        ctx.storage,
                        (liquidation_price.into(), pos.id),
                        &LiquidationReason::Liquidated,
                    )?;
                }

                if let Some(take_profit) = pos.take_profit_price {
                    PRICE_TRIGGER_ASC.save(
                        ctx.storage,
                        (take_profit.into(), pos.id),
                        &LiquidationReason::MaxGains,
                    )?;
                }

                if let Some(stop_loss_override) = pos.stop_loss_override_notional {
                    PRICE_TRIGGER_DESC.save(
                        ctx.storage,
                        (stop_loss_override.into(), pos.id),
                        &LiquidationReason::StopLoss,
                    )?;
                }

                if let Some(take_profit_override) = pos.take_profit_override_notional {
                    PRICE_TRIGGER_ASC.save(
                        ctx.storage,
                        (take_profit_override.into(), pos.id),
                        &LiquidationReason::TakeProfit,
                    )?;
                }
            }
            DirectionToNotional::Short => {
                if let Some(liquidation_price) = pos.liquidation_price {
                    PRICE_TRIGGER_ASC.save(
                        ctx.storage,
                        (liquidation_price.into(), pos.id),
                        &LiquidationReason::Liquidated,
                    )?;
                }

                if let Some(take_profit_price) = pos.take_profit_price {
                    PRICE_TRIGGER_DESC.save(
                        ctx.storage,
                        (take_profit_price.into(), pos.id),
                        &LiquidationReason::MaxGains,
                    )?;
                }

                if let Some(stop_loss_override) = pos.stop_loss_override_notional {
                    PRICE_TRIGGER_ASC.save(
                        ctx.storage,
                        (stop_loss_override.into(), pos.id),
                        &LiquidationReason::StopLoss,
                    )?;
                }

                if let Some(take_profit_override) = pos.take_profit_override_notional {
                    PRICE_TRIGGER_DESC.save(
                        ctx.storage,
                        (take_profit_override.into(), pos.id),
                        &LiquidationReason::TakeProfit,
                    )?;
                }
            }
        };

        Ok(())
    }
}

/// Result of checking if we can adjust net open interest
#[must_use]
pub(crate) enum AdjustOpenInterestResult {
    /// Set the long interest to this value
    Long(Notional),
    /// Set the short interest to this value
    Short(Notional),
}

impl AdjustOpenInterestResult {
    pub(crate) fn net_notional(
        &self,
        state: &State,
        store: &dyn Storage,
    ) -> Result<Signed<Notional>> {
        Ok(match self {
            AdjustOpenInterestResult::Long(long) => {
                long.into_signed() - state.open_short_interest(store)?.into_signed()
            }
            AdjustOpenInterestResult::Short(short) => {
                state.open_long_interest(store)?.into_signed() - short.into_signed()
            }
        })
    }
    pub(crate) fn store(&self, ctx: &mut StateContext) -> Result<()> {
        match self {
            AdjustOpenInterestResult::Long(long) => {
                OPEN_NOTIONAL_LONG_INTEREST.save(ctx.storage, long)?
            }
            AdjustOpenInterestResult::Short(short) => {
                OPEN_NOTIONAL_SHORT_INTEREST.save(ctx.storage, short)?
            }
        }
        Ok(())
    }
}
