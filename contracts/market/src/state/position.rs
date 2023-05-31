mod liquifund;

use cosmwasm_std::Order;
pub use liquifund::*;
mod open;
use msg::contracts::market::{
    entry::{ClosedPositionCursor, ClosedPositionsResp},
    position::events::{PositionSaveEvent, PositionSaveReason},
};
pub(crate) use open::*;
mod close;
pub use close::*;
mod update;
pub use update::*;
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

/// Positions which need to be added to the liquidation/take profit price maps when cranking.
///
/// These are deferred to ensure that we don't liquidate a position if the crank
/// is falling behind. It's possible that an old price may trigger
/// liquidation/take profit. Instead, we only insert into the maps above once
/// the entry price timestamp has been hit.
///
/// Key is the timestamp of the last time the liquidation prices were set (see
/// [LIQUIDATION_PRICES_PENDING_REVERSE]) and position ID.
pub(super) const LIQUIDATION_PRICES_PENDING: Map<(Timestamp, PositionId), ()> =
    Map::new(namespace::LIQUIDATION_PRICES_PENDING);

/// How many positions are sitting in the pending queue.
///
/// Note that during migration of data, this field may start off empty. The rest
/// of the code needs to account for that possibility, and if trying to subtract
/// from 0, should simply provide 0 as the value here.
pub(super) const LIQUIDATION_PRICES_PENDING_COUNT: Item<u32> =
    Item::new(namespace::LIQUIDATION_PRICES_PENDING_COUNT);

/// Timestamp of the last time the liquidation prices were set.
pub(super) const LIQUIDATION_PRICES_PENDING_REVERSE: Map<PositionId, Timestamp> =
    Map::new(namespace::LIQUIDATION_PRICES_PENDING_REVERSE);

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

/// Tracks when the protocol will next be stale vis-a-vis pending liquifunding.
///
/// It would seem like we could check that by using [NEXT_LIQUIFUNDING] and
/// adding in the staleness duration. However, if the staleness period
/// configuration changes after liquifunding, that calculation will no longer
/// guarantee well-fundedness. Instead, we track "when will we go stale" when
/// setting up liquidation margin initially.
pub(super) const NEXT_STALE: Map<(Timestamp, PositionId), ()> = Map::new(namespace::NEXT_STALE);

pub enum PositionOrId {
    Id(PositionId),
    Pos(Box<Position>),
}

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

impl PositionOrId {
    pub(crate) fn extract(self, store: &dyn Storage) -> Result<Position> {
        match self {
            PositionOrId::Id(id) => get_position(store, id),
            PositionOrId::Pos(pos) => Ok(*pos),
        }
    }
}

fn already(dir: DirectionToBase) -> MarketError {
    match dir {
        DirectionToBase::Long => MarketError::DeltaNeutralityFeeAlreadyLong {},
        DirectionToBase::Short => MarketError::DeltaNeutralityFeeAlreadyShort {},
    }
}

fn newly(dir: DirectionToBase) -> MarketError {
    match dir {
        DirectionToBase::Long => MarketError::DeltaNeutralityFeeNewlyLong {},
        DirectionToBase::Short => MarketError::DeltaNeutralityFeeNewlyShort {},
    }
}

fn flipped(dir: DirectionToBase) -> MarketError {
    match dir {
        DirectionToBase::Long => MarketError::DeltaNeutralityFeeShortToLong {},
        DirectionToBase::Short => MarketError::DeltaNeutralityFeeLongToShort {},
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
                    DirectionToNotional::Short => Err(already(base_direction)),
                    // We don't allow the user to swing the market all the way from capped low to capped high
                    DirectionToNotional::Long => {
                        if is_capped_high_after {
                            Err(flipped(base_direction))
                        } else {
                            Ok(())
                        }
                    }
                }
            } else if is_capped_high_before {
                match notional_direction {
                    // We were already too long, disallow going longer
                    DirectionToNotional::Long => Err(already(base_direction)),
                    // We don't allow the user to swing the market all the way from capped high to capped low
                    DirectionToNotional::Short => {
                        if is_capped_low_after {
                            Err(flipped(base_direction))
                        } else {
                            Ok(())
                        }
                    }
                }
            } else if is_capped_low_after {
                debug_assert!(notional_size_diff <= Signed::zero());
                Err(newly(base_direction))
            } else if is_capped_high_after {
                debug_assert!(notional_size_diff >= Signed::zero());
                Err(newly(base_direction))
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
        calc_pending_fees: bool,
    ) -> Result<PositionOrPendingClose> {
        let config = &self.config;
        let market_type = self.market_id(store)?.get_market_type();
        let entry_price = match self.spot_price(store, Some(pos.created_at)) {
            Ok(entry_price) => entry_price,
            Err(err) => return Err(err),
        };
        let spot_price = self.spot_price(store, None)?;

        // PERP-996 ensure we do not flip direction, see comments in
        // liquifunding for more details
        let original_direction_to_base = pos
            .active_leverage_to_notional(&spot_price)
            .into_base(market_type)
            .split()
            .0;

        let dnf_on_close_collateral = if calc_pending_fees {
            // Calculate pending fees
            let (borrow_fees, _) =
                self.calc_capped_borrow_fee_payment(store, &pos, pos.liquifunded_at, self.now())?;
            let borrow_fees = borrow_fees.lp.checked_add(borrow_fees.xlp)?;
            let (funding_payments, _) =
                self.calc_capped_funding_payment(store, &pos, pos.liquifunded_at, self.now())?;
            let delta_neutrality_fee = self.calc_delta_neutrality_fee(
                store,
                -pos.notional_size,
                spot_price,
                Some(pos.liquidation_margin.delta_neutrality),
            )?;
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
            Some(delta_neutrality_fee)
        } else {
            None
        };

        let start_price = self.spot_price(store, Some(pos.liquifunded_at))?;
        pos.into_query_response_extrapolate_exposure(
            start_price.price_notional,
            spot_price,
            entry_price.price_notional,
            &spot_price,
            config,
            market_type,
            original_direction_to_base,
            dnf_on_close_collateral,
        )
    }

    pub(crate) fn position_assert_owner(
        &self,
        store: &dyn Storage,
        pos_or_id: PositionOrId,
        addr: &Addr,
    ) -> Result<()> {
        match pos_or_id.extract(store) {
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

    /// Check if there is a pending liquidation price that can be added to the real data structures.
    pub(crate) fn pending_liquidation_prices(
        &self,
        store: &dyn Storage,
        price_point_timestamp: Timestamp,
    ) -> Result<Option<PositionId>> {
        Ok(LIQUIDATION_PRICES_PENDING
            .keys(store, None, None, Order::Ascending)
            .next()
            .transpose()?
            .and_then(|(updated_at, pos)| {
                if updated_at <= price_point_timestamp {
                    Some(pos)
                } else {
                    None
                }
            }))
    }

    /// Decrement the pending position count by one.
    fn decrement_pending_count(&self, ctx: &mut StateContext) -> Result<()> {
        let new = LIQUIDATION_PRICES_PENDING_COUNT
            .may_load(ctx.storage)?
            .unwrap_or_default()
            .checked_sub(1)
            .unwrap_or_default();
        LIQUIDATION_PRICES_PENDING_COUNT.save(ctx.storage, &new)?;
        Ok(())
    }

    /// Increment the pending count
    ///
    /// If this is a user driven action (like open or update), we block this
    /// action if we've already hit our congestion limit. For automated actions
    /// like cranked liquifundings, we always let the unpending occur.
    fn increment_pending_count(
        &self,
        ctx: &mut StateContext,
        reason: PositionSaveReason,
    ) -> Result<()> {
        let old = LIQUIDATION_PRICES_PENDING_COUNT
            .may_load(ctx.storage)?
            .unwrap_or_default();

        if old >= self.config.unpend_limit {
            if let Some(reason) = reason.into_congestion_reason() {
                return Err(MarketError::Congestion {
                    current_queue: old,
                    max_size: self.config.unpend_limit,
                    reason,
                }
                .into_anyhow());
            }
        }

        // If we hit the numeric overflow, then (1) that's insane and (2) just keep the old value, we're allowed to undercount this.
        if let Some(new) = old.checked_add(1) {
            LIQUIDATION_PRICES_PENDING_COUNT.save(ctx.storage, &new)?;
        }
        Ok(())
    }

    /// Ensure that we're not in a congested state.
    ///
    /// This is used to prevent users from placing more limit orders while the
    /// market is congested. Placing additional limit orders can open a spam
    /// attack vector when the market is in the congested state.
    pub(crate) fn ensure_not_congested(
        &self,
        store: &dyn Storage,
        reason: CongestionReason,
    ) -> Result<()> {
        let count = LIQUIDATION_PRICES_PENDING_COUNT
            .may_load(store)?
            .unwrap_or_default();

        if count >= self.config.unpend_limit {
            Err(MarketError::Congestion {
                current_queue: count,
                max_size: self.config.unpend_limit,
                reason,
            }
            .into_anyhow())
        } else {
            Ok(())
        }
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
        debug_assert!(NEXT_STALE.has(ctx.storage, (position.stale_at, position.id)));

        OPEN_POSITIONS.remove(ctx.storage, position.id);
        NEXT_LIQUIFUNDING.remove(ctx.storage, (position.next_liquifunding, position.id));
        NEXT_STALE.remove(ctx.storage, (position.stale_at, position.id));

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
            pos.liquidation_margin = pos.liquidation_margin(
                price_point.price_notional,
                &self.spot_price(ctx.storage, None)?,
                &self.config,
            )?;
        } else {
            debug_assert_eq!(
                pos.liquidation_margin,
                pos.liquidation_margin(
                    price_point.price_notional,
                    &self.spot_price(ctx.storage, None)?,
                    &self.config
                )?
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
        debug_assert!(pos.next_liquifunding < pos.stale_at);

        OPEN_POSITIONS.save(ctx.storage, pos.id, pos)?;
        NEXT_LIQUIFUNDING.save(ctx.storage, (pos.next_liquifunding, pos.id), &())?;
        NEXT_STALE.save(ctx.storage, (pos.stale_at, pos.id), &())?;
        let used_pending_queue = self.store_liquidation_prices(ctx, pos, reason)?;

        self.increase_total_funding_margin(ctx, pos.liquidation_margin.funding)?;

        ctx.response.add_event(PositionSaveEvent {
            id: pos.id,
            reason,
            used_pending_queue,
        });

        Ok(())
    }

    /// Removes a position's liquidation price and take profit prices
    fn remove_liquidation_prices(&self, ctx: &mut StateContext, pos: &Position) -> Result<()> {
        if let Some(updated_at) =
            LIQUIDATION_PRICES_PENDING_REVERSE.may_load(ctx.storage, pos.id)?
        {
            if LIQUIDATION_PRICES_PENDING.has(ctx.storage, (updated_at, pos.id)) {
                self.decrement_pending_count(ctx)?;
                LIQUIDATION_PRICES_PENDING_REVERSE.remove(ctx.storage, pos.id);
                LIQUIDATION_PRICES_PENDING.remove(ctx.storage, (updated_at, pos.id));
            } else {
                debug_assert!(!LIQUIDATION_PRICES_PENDING_REVERSE.has(ctx.storage, pos.id))
            }
        }

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

    /// Stores a position's liquidation price and take profit prices for easy processing.
    /// Implicitly removes existing liquidation prices for the specified position.
    ///
    /// Param `spot_price` represents the spot price at the timestamp for which the liquidation prices should
    /// be calculated. For example, for newly open positions, this is the timestamp at which the position was
    /// opened.
    ///
    /// If the crank is currently up to date, this function will immediately
    /// store the liquidation prices for price trigger processing. However, if
    /// the crank is lagging behind, we instead put the prices on the on the
    /// [LIQUIDATION_PRICES_PENDING] queue so that historical price updates
    /// can't trigger liquidation/take profit.  Actually adding them will then
    /// occur in the crank.
    ///
    /// Returns [true] if we used the unpend queue, [false] otherwise.
    fn store_liquidation_prices(
        &self,
        ctx: &mut StateContext,
        pos: &Position,
        reason: PositionSaveReason,
    ) -> Result<bool> {
        if self.is_crank_up_to_date(ctx.storage)? {
            self.store_liquidation_prices_inner(ctx, pos)?;
            Ok(false)
        } else {
            let now = self.now();
            LIQUIDATION_PRICES_PENDING_REVERSE.save(ctx.storage, pos.id, &now)?;
            LIQUIDATION_PRICES_PENDING.save(ctx.storage, (now, pos.id), &())?;
            self.increment_pending_count(ctx, reason)?;
            Ok(true)
        }
    }

    /// Take a single position from [LIQUIDATION_PRICES_PENDING] and moves it to the real data structures.
    pub(super) fn unpend_liquidation_prices(
        &self,
        ctx: &mut StateContext,
        posid: PositionId,
    ) -> Result<()> {
        let updated_at = LIQUIDATION_PRICES_PENDING_REVERSE.load(ctx.storage, posid)?;
        LIQUIDATION_PRICES_PENDING_REVERSE.remove(ctx.storage, posid);
        LIQUIDATION_PRICES_PENDING.remove(ctx.storage, (updated_at, posid));
        self.decrement_pending_count(ctx)?;

        let pos = OPEN_POSITIONS.load(ctx.storage, posid)?;
        self.store_liquidation_prices_inner(ctx, &pos)
    }

    /// Actually store the liquidation prices
    ///
    /// This can either happen because we tried to store new prices and the
    /// protocol's crank was up to date, _or_ because the protocol was lagging
    /// behind on the crank and we're now unpending a queued liquidation price.
    fn store_liquidation_prices_inner(&self, ctx: &mut StateContext, pos: &Position) -> Result<()> {
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
    pub(crate) fn store(self, ctx: &mut StateContext) -> Result<()> {
        match self {
            AdjustOpenInterestResult::Long(long) => {
                OPEN_NOTIONAL_LONG_INTEREST.save(ctx.storage, &long)?
            }
            AdjustOpenInterestResult::Short(short) => {
                OPEN_NOTIONAL_SHORT_INTEREST.save(ctx.storage, &short)?
            }
        }
        Ok(())
    }
}
