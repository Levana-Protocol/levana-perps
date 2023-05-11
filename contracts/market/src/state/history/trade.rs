use crate::prelude::*;
use crate::state::{State, StateContext};
use cosmwasm_std::{Addr, Order, Storage};
use cw_storage_plus::{Bound, KeyDeserialize, Map, Prefixer, PrimaryKey};
use msg::contracts::market::entry::TraderActionHistoryResp;
use msg::contracts::market::{
    entry::{PositionAction, PositionActionHistoryResp, PositionActionKind, TradeHistorySummary},
    history::events::{PnlEvent, PositionActionEvent, TradeVolumeEvent},
};
use shared::storage::push_to_monotonic_multilevel_map;

const TRADE_HISTORY_SUMMARY: Map<&Addr, TradeHistorySummary> =
    Map::new(namespace::TRADE_HISTORY_SUMMARY);
const TRADE_HISTORY_BY_POSITION: Map<(PositionId, u64), PositionAction> =
    Map::new(namespace::TRADE_HISTORY_BY_POSITION);
const TRADE_HISTORY_BY_ADDRESS: Map<(&Addr, u64), PositionAction> =
    Map::new(namespace::TRADE_HISTORY_BY_ADDRESS);

/// Calculate the volume of the position for trade volume.
///
/// This is similar to, but distinct from, notional size. Notional size will
/// have different leverage numbers due to the 1-leverage conversion between
/// base and notional. This calculation is intended to convert back to the
/// user-facing leverage (in base) numbers.
pub fn trade_volume_usd(
    pos: &Position,
    price_point: PricePoint,
    market_type: MarketType,
) -> Result<Usd> {
    let leverage = pos
        .active_leverage_to_notional(&price_point)
        .into_base(market_type)
        .split()
        .1;
    let trade_volume_collateral = pos
        .active_collateral
        .raw()
        .checked_mul_dec(leverage.raw().into_decimal256())?;
    Ok(price_point.collateral_to_usd(trade_volume_collateral))
}

/// Same as [trade_volume_usd], but uses [ClosedPosition] for its data instead.
fn trade_volume_usd_from_closed(pos: &ClosedPosition, price_point: &PricePoint) -> Result<Usd> {
    let collateral_factor = match price_point.market_type {
        MarketType::CollateralIsQuote => Notional::from(0u64).into_signed(),
        MarketType::CollateralIsBase => match pos.direction_to_base {
            DirectionToBase::Long => price_point
                .collateral_to_notional(pos.active_collateral)
                .into_signed(),
            DirectionToBase::Short => -price_point
                .collateral_to_notional(pos.active_collateral)
                .into_signed(),
        },
    };
    let trade_volume_notional = pos.notional_size + collateral_factor;
    Ok(price_point.notional_to_usd(trade_volume_notional.abs_unsigned()))
}

impl State<'_> {
    pub(crate) fn position_history_add_close(
        &self,
        ctx: &mut StateContext,
        pos: &ClosedPosition,
        delta_netruality_fee: Signed<Collateral>,
        settlement_price: &PricePoint,
    ) -> Result<()> {
        self.position_history_add_close_action(
            ctx,
            pos,
            pos.active_collateral,
            delta_netruality_fee,
            settlement_price,
        )?;
        self.trade_history_add_volume(
            ctx,
            &pos.owner,
            trade_volume_usd_from_closed(pos, settlement_price)?,
        )?;
        self.trade_history_add_realized_pnl(ctx, &pos.owner, pos.pnl_collateral, pos.pnl_usd)?;

        Ok(())
    }

    pub(crate) fn position_history_add_transfer(
        &self,
        ctx: &mut StateContext,
        pos: &Position,
        old_owner: Addr,
    ) -> Result<()> {
        let action = PositionAction {
            id: Some(pos.id),
            kind: PositionActionKind::Transfer,
            timestamp: self.now(),
            collateral: pos.active_collateral.raw(),
            leverage: None,
            max_gains: None,
            trade_fee: None,
            delta_neutrality_fee: None,
            old_owner: Some(old_owner.clone()),
            new_owner: Some(pos.owner.clone()),
        };

        ctx.response.add_event(PositionActionEvent {
            pos_id: pos.id,
            action: action.clone(),
        });

        push_to_monotonic_multilevel_map(ctx.storage, TRADE_HISTORY_BY_POSITION, pos.id, &action)?;
        push_to_monotonic_multilevel_map(
            ctx.storage,
            TRADE_HISTORY_BY_ADDRESS,
            &pos.owner,
            &action,
        )?;
        push_to_monotonic_multilevel_map(
            ctx.storage,
            TRADE_HISTORY_BY_ADDRESS,
            &old_owner,
            &action,
        )?;

        Ok(())
    }

    pub(crate) fn trade_history_get_summary(
        &self,
        store: &dyn Storage,
        addr: &Addr,
    ) -> Result<TradeHistorySummary> {
        Ok(TRADE_HISTORY_SUMMARY
            .may_load(store, addr)?
            .unwrap_or_default())
    }

    pub(crate) fn get_history_helper<'a, K, T>(
        &self,
        map: Map<'a, (K, u64), T>,
        store: &dyn Storage,
        id: K,
        start_after: Option<u64>,
        limit: Option<u32>,
        order: Option<Order>,
    ) -> Result<(Vec<T>, Option<String>)>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
        K: PrimaryKey<'a> + Prefixer<'a> + KeyDeserialize,
    {
        let mut iter = map.prefix(id).range(
            store,
            start_after.map(Bound::exclusive),
            None,
            order.unwrap_or(Order::Ascending),
        );
        const MAX_LIMIT: u32 = 20;
        let limit = limit.unwrap_or(MAX_LIMIT).min(MAX_LIMIT).try_into()?;
        let mut actions = Vec::with_capacity(limit);
        let mut next_start_after = None;
        for _ in 0..limit {
            match iter.next().transpose()? {
                Some((k, v)) => {
                    actions.push(v);
                    next_start_after = Some(k);
                }
                None => {
                    next_start_after = None;
                    break;
                }
            }
        }

        if next_start_after.is_some() && iter.next().is_none() {
            next_start_after = None;
        }

        Ok((actions, next_start_after.map(|x| x.to_string())))
    }

    pub(crate) fn position_action_get_history(
        &self,
        store: &dyn Storage,
        id: PositionId,
        start_after: Option<u64>,
        limit: Option<u32>,
        order: Option<Order>,
    ) -> Result<PositionActionHistoryResp> {
        let (actions, next_start_after) = self.get_history_helper(
            TRADE_HISTORY_BY_POSITION,
            store,
            id,
            start_after,
            limit,
            order,
        )?;
        Ok(PositionActionHistoryResp {
            actions,
            next_start_after,
        })
    }

    pub(crate) fn trader_action_get_history(
        &self,
        store: &dyn Storage,
        owner: &Addr,
        start_after: Option<u64>,
        limit: Option<u32>,
        order: Option<Order>,
    ) -> Result<TraderActionHistoryResp> {
        let (actions, next_start_after) = self.get_history_helper(
            TRADE_HISTORY_BY_ADDRESS,
            store,
            owner,
            start_after,
            limit,
            order,
        )?;
        Ok(TraderActionHistoryResp {
            actions,
            next_start_after,
        })
    }

    pub(crate) fn position_history_add_action(
        &self,
        ctx: &mut StateContext,
        pos: &Position,
        kind: PositionActionKind,
        trading_fee: Option<Collateral>,
        delta_netruality_fee: Option<Signed<Collateral>>,
        price_point: PricePoint,
    ) -> Result<()> {
        let market_type = self.market_type(ctx.storage)?;
        let leverage = pos
            .active_leverage_to_notional(&price_point)
            .into_base(market_type)
            .split()
            .1;
        let trade_fee_usd = trading_fee.map(|x| price_point.collateral_to_usd(x));

        let action = PositionAction {
            id: Some(pos.id),
            kind,
            timestamp: self.now(),
            collateral: pos.active_collateral.raw(),
            leverage: Some(leverage),
            max_gains: Some(pos.max_gains_in_quote(market_type, price_point)?),
            trade_fee: trade_fee_usd,
            delta_neutrality_fee: delta_netruality_fee
                .map(|x| x.map(|x| price_point.collateral_to_usd(x))),
            old_owner: None,
            new_owner: None,
        };

        ctx.response.add_event(PositionActionEvent {
            pos_id: pos.id,
            action: action.clone(),
        });

        push_to_monotonic_multilevel_map(ctx.storage, TRADE_HISTORY_BY_POSITION, pos.id, &action)?;
        push_to_monotonic_multilevel_map(
            ctx.storage,
            TRADE_HISTORY_BY_ADDRESS,
            &pos.owner,
            &action,
        )?;

        Ok(())
    }

    fn position_history_add_close_action(
        &self,
        ctx: &mut StateContext,
        pos: &ClosedPosition,
        active_collateral: Collateral,
        delta_netruality_fee: Signed<Collateral>,
        price_point: &PricePoint,
    ) -> Result<()> {
        let action = PositionAction {
            id: Some(pos.id),
            kind: PositionActionKind::Close,
            timestamp: self.now(),
            collateral: active_collateral,
            leverage: None,
            max_gains: None,
            trade_fee: None,
            delta_neutrality_fee: Some(
                delta_netruality_fee.map(|x| price_point.collateral_to_usd(x)),
            ),
            old_owner: None,
            new_owner: None,
        };

        ctx.response.add_event(PositionActionEvent {
            pos_id: pos.id,
            action: action.clone(),
        });

        push_to_monotonic_multilevel_map(ctx.storage, TRADE_HISTORY_BY_POSITION, pos.id, &action)?;
        push_to_monotonic_multilevel_map(
            ctx.storage,
            TRADE_HISTORY_BY_ADDRESS,
            &pos.owner,
            &action,
        )?;

        Ok(())
    }

    pub(crate) fn trade_history_add_volume(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        volume_usd: Usd,
    ) -> Result<()> {
        let mut summary = self.trade_history_get_summary(ctx.storage, addr)?;

        summary.trade_volume = summary.trade_volume.checked_add(volume_usd)?;
        TRADE_HISTORY_SUMMARY.save(ctx.storage, addr, &summary)?;

        ctx.response.add_event(TradeVolumeEvent { volume_usd });

        Ok(())
    }

    fn trade_history_add_realized_pnl(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        pnl: Signed<Collateral>,
        pnl_usd: Signed<Usd>,
    ) -> Result<()> {
        let mut summary = self.trade_history_get_summary(ctx.storage, addr)?;

        summary.realized_pnl += pnl_usd;
        TRADE_HISTORY_SUMMARY.save(ctx.storage, addr, &summary)?;

        ctx.response.add_event(PnlEvent { pnl, pnl_usd });

        Ok(())
    }
}
