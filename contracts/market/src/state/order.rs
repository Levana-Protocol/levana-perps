use crate::state::{State, StateContext};
use anyhow::{Context, Result};
use cosmwasm_std::{Addr, Order, Storage};
use cw_storage_plus::{Bound, Item, Map, PrefixBound};
use msg::contracts::market::entry::{
    ExecutedLimitOrder, LimitOrderHistoryResp, LimitOrderResp, LimitOrderResult, LimitOrdersResp,
};
use msg::contracts::market::fees::events::TradeId;
use msg::contracts::market::order::events::{
    CancelLimitOrderEvent, ExecuteLimitOrderEvent, PlaceLimitOrderEvent,
};
use msg::contracts::market::order::{LimitOrder, OrderId};
use msg::contracts::market::position::CollateralAndUsd;
use msg::prelude::*;

use super::position::OpenPositionParams;

/// Stores the last used [OrderId]
const LAST_ORDER_ID: Item<OrderId> = Item::new(namespace::LAST_ORDER_ID);
/// Stores [LimitOrder]s by OrderId
const LIMIT_ORDERS: Map<OrderId, LimitOrder> = Map::new(namespace::LIMIT_ORDERS);
/// Indexes long [LimitOrder]s by trigger price
const LIMIT_ORDERS_BY_PRICE_LONG: Map<(PriceKey, OrderId), ()> =
    Map::new(namespace::LIMIT_ORDERS_BY_PRICE_LONG);
/// Indexes short [LimitOrder]s by trigger price
const LIMIT_ORDERS_BY_PRICE_SHORT: Map<(PriceKey, OrderId), ()> =
    Map::new(namespace::LIMIT_ORDERS_BY_PRICE_SHORT);
/// Indexes [LimitOrder]s by Addr
const LIMIT_ORDERS_BY_ADDR: Map<(&Addr, OrderId), ()> = Map::new(namespace::LIMIT_ORDERS_BY_ADDR);
/// Executed limit orders for history.
///
/// The [u64] is an [OrderId]. We use [u64] to match the rest of the history API helper functions.
const EXECUTED_LIMIT_ORDERS: Map<(&Addr, u64), ExecutedLimitOrder> =
    Map::new(namespace::EXECUTED_LIMIT_ORDERS);

impl State<'_> {
    /// Sets a [LimitOrder]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn limit_order_set_order(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        trigger_price: PriceBaseInQuote,
        collateral: NonZero<Collateral>,
        leverage: LeverageToBase,
        direction: DirectionToNotional,
        max_gains: MaxGainsInQuote,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
        deferred_exec_crank_fee: Collateral,
        deferred_exec_crank_fee_usd: Usd,
        price: &PricePoint,
    ) -> Result<OrderId> {
        let last_order_id = LAST_ORDER_ID
            .may_load(ctx.storage)?
            .unwrap_or_else(|| OrderId::new(0));
        let order_id = OrderId::new(last_order_id.u64() + 1);
        LAST_ORDER_ID.save(ctx.storage, &order_id)?;

        let crank_fee_usd = self.config.crank_fee_charged;
        let crank_fee = price.usd_to_collateral(crank_fee_usd);
        self.collect_crank_fee(ctx, TradeId::LimitOrder(order_id), crank_fee, crank_fee_usd)?;
        let collateral = collateral
            .checked_sub(crank_fee)
            .context("Insufficient funds to cover fees, failed on crank fee")?;

        let order = LimitOrder {
            order_id,
            owner: owner.clone(),
            trigger_price,
            collateral,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
            crank_fee_collateral: crank_fee.checked_add(deferred_exec_crank_fee)?,
            crank_fee_usd: crank_fee_usd.checked_add(deferred_exec_crank_fee_usd)?,
        };

        LIMIT_ORDERS.save(ctx.storage, order_id, &order)?;

        let market_type = self.market_type(ctx.storage)?;
        match direction {
            DirectionToNotional::Long => LIMIT_ORDERS_BY_PRICE_LONG.save(
                ctx.storage,
                (trigger_price.into_price_key(market_type), order_id),
                &(),
            )?,
            DirectionToNotional::Short => LIMIT_ORDERS_BY_PRICE_SHORT.save(
                ctx.storage,
                (trigger_price.into_price_key(market_type), order_id),
                &(),
            )?,
        }

        LIMIT_ORDERS_BY_ADDR.save(ctx.storage, (&owner, order_id), &())?;

        let direction_to_base = direction.into_base(market_type);
        ctx.response.add_event(PlaceLimitOrderEvent {
            market_type,
            collateral: order.collateral,
            collateral_usd: price.collateral_to_usd_non_zero(collateral),
            leverage: order.leverage.into_signed(direction_to_base),
            direction: direction_to_base,
            max_gains,
            stop_loss_override,
            order_id,
            owner,
            trigger_price,
            take_profit_override,
        });

        Ok(order_id)
    }

    /// Returns the next long or short [LimitOrder] whose trigger price is above the specified price
    /// for long orders or below the specified price for short orders.
    ///
    /// The provided price comes from the current price point we're cranking, or
    /// from a queried price to check if a price update would lead to a trigger or limit
    /// order being available.
    pub(crate) fn limit_order_triggered_order(
        &self,
        storage: &dyn Storage,
        price: Price,
    ) -> Result<Option<OrderId>> {
        let order = LIMIT_ORDERS_BY_PRICE_LONG
            .prefix_range(
                storage,
                Some(PrefixBound::inclusive(PriceKey::from(price))),
                None,
                // If we had a continuous price stream with no holes whatsoever, the higher price here would have already been triggered
                // But since we have holes, we want to walk in descending order to find the "most urgent" price to execute first 
                // i.e. we start with the ones which are furthest away from our target price, and work our way inwards
                Order::Descending,
            )
            .next();

        let order = match order {
            Some(_) => order,
            None => LIMIT_ORDERS_BY_PRICE_SHORT
                .prefix_range(
                    storage,
                    None,
                    Some(PrefixBound::inclusive(PriceKey::from(price))),
                    Order::Ascending,
                )
                .next(),
        };

        match order {
            None => Ok(None),
            Some(res) => {
                let ((_, order_id), ()) = res?;
                Ok(Some(order_id))
            }
        }
    }

    /// Would the given new price cause a new limit order to be triggerable?
    pub(crate) fn limit_order_newly_triggered_order(
        &self,
        storage: &dyn Storage,
        oracle_price: Price,
        new_price: Price,
    ) -> bool {
        LIMIT_ORDERS_BY_PRICE_LONG
            .prefix_range(
                storage,
                Some(PrefixBound::inclusive(PriceKey::from(new_price))),
                Some(PrefixBound::exclusive(PriceKey::from(oracle_price))),
                Order::Ascending,
            )
            .next()
            .is_some()
            || LIMIT_ORDERS_BY_PRICE_SHORT
                .prefix_range(
                    storage,
                    Some(PrefixBound::exclusive(PriceKey::from(oracle_price))),
                    Some(PrefixBound::inclusive(PriceKey::from(new_price))),
                    Order::Descending,
                )
                .next()
                .is_some()
    }

    /// Attempts to execute the specified limit order by opening a position.
    /// If the position fails to open, the limit order is removed from the protocol.
    pub(crate) fn limit_order_execute_order(
        &self,
        ctx: &mut StateContext,
        order_id: OrderId,
        price_point: &PricePoint,
    ) -> Result<()> {
        let order = LIMIT_ORDERS.load(ctx.storage, order_id)?;
        self.limit_order_remove(ctx.storage, &order)?;

        #[cfg(debug_assertions)]
        {
            let trigger = order
                .trigger_price
                .into_notional_price(price_point.market_type);
            match order.direction {
                DirectionToNotional::Long => debug_assert!(trigger >= price_point.price_notional),
                DirectionToNotional::Short => {
                    debug_assert!(trigger <= price_point.price_notional)
                }
            }
        }

        let market_type = self.market_type(ctx.storage)?;

        let open_position_params = OpenPositionParams {
            owner: order.owner.clone(),
            collateral: order.collateral,
            crank_fee: CollateralAndUsd::from_pair(order.crank_fee_collateral, order.crank_fee_usd),
            leverage: order.leverage,
            direction: order.direction.into_base(market_type),
            max_gains_in_quote: order.max_gains,
            slippage_assert: None,
            stop_loss_override: order.stop_loss_override,
            take_profit_override: order.take_profit_override,
        };
        let res = self.validate_new_position(ctx.storage, open_position_params, price_point);

        let res = match res {
            Ok(validated_position) => {
                let pos_id = self.open_validated_position(ctx, validated_position, false)?;
                Ok(pos_id)
            }
            Err(e) => {
                self.add_token_transfer_msg(ctx, &order.owner, order.collateral)?;
                Err(e)
            }
        };

        ctx.response.add_event(ExecuteLimitOrderEvent {
            order_id,
            pos_id: res.as_ref().ok().copied(),
            error: res.as_ref().err().map(|e| e.to_string()),
        });

        EXECUTED_LIMIT_ORDERS.save(
            ctx.storage,
            (&order.owner, order_id.u64()),
            &ExecutedLimitOrder {
                order: order.clone(),
                result: match res {
                    Ok(position) => LimitOrderResult::Success { position },
                    Err(e) => LimitOrderResult::Failure {
                        reason: format!("{e:?}"),
                    },
                },
                timestamp: self.now(),
            },
        )?;

        Ok(())
    }

    /// Loads a single [LimitOrder] by [OrderId]
    pub(crate) fn limit_order_load(
        &self,
        storage: &dyn Storage,
        order_id: OrderId,
    ) -> Result<LimitOrder> {
        Ok(LIMIT_ORDERS.load(storage, order_id)?)
    }

    /// Loads all [LimitOrder]s. Available in debug only.
    #[cfg(feature = "sanity")]
    pub(crate) fn limit_order_load_all(&self, storage: &dyn Storage) -> Result<Vec<LimitOrder>> {
        let orders = LIMIT_ORDERS
            .range(storage, None, None, Order::Ascending)
            .map(|res| res.map(|order| order.1))
            .collect::<Result<Vec<LimitOrder>, _>>()?;

        Ok(orders)
    }

    /// Loads [LimitOrder]s by Addr
    pub(crate) fn limit_order_load_by_addr(
        &self,
        storage: &dyn Storage,
        addr: Addr,
        start_after: Option<OrderId>,
        limit: Option<u32>,
        order: Option<Order>,
    ) -> Result<LimitOrdersResp> {
        let mut iter = LIMIT_ORDERS_BY_ADDR.prefix(&addr).range(
            storage,
            start_after.map(Bound::exclusive),
            None,
            order.unwrap_or(Order::Ascending),
        );

        const MAX_LIMIT: u32 = 20;
        let limit = limit.unwrap_or(MAX_LIMIT).min(MAX_LIMIT).try_into()?;
        let mut orders = Vec::with_capacity(limit);
        let mut next_start_after = None;
        for _ in 0..limit {
            match iter.next().transpose()? {
                Some((order_id, _)) => {
                    let order = LIMIT_ORDERS.load(storage, order_id)?;
                    let market_type = self.market_type(storage)?;
                    let order_resp = LimitOrderResp {
                        order_id,
                        trigger_price: order.trigger_price,
                        collateral: order.collateral,
                        leverage: order.leverage,
                        direction: order.direction.into_base(market_type),
                        max_gains: order.max_gains,
                        stop_loss_override: order.stop_loss_override,
                        take_profit_override: order.take_profit_override,
                    };

                    orders.push(order_resp);
                    next_start_after = Some(order_id);
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

        Ok(LimitOrdersResp {
            orders,
            next_start_after,
        })
    }

    /// Validates that the specified Addr is the owner of the [LimitOrder]
    pub(crate) fn limit_order_assert_owner(
        &self,
        storage: &dyn Storage,
        owner: &Addr,
        order_id: OrderId,
    ) -> Result<()> {
        anyhow::ensure!(
            LIMIT_ORDERS_BY_ADDR.has(storage, (owner, order_id)),
            "Limit order {} is not owned by {}",
            order_id,
            owner
        );

        Ok(())
    }

    /// Cancels a limit order
    pub(crate) fn limit_order_cancel_order(
        &self,
        ctx: &mut StateContext,
        order_id: OrderId,
    ) -> Result<()> {
        let order = LIMIT_ORDERS.load(ctx.storage, order_id)?;
        self.limit_order_remove(ctx.storage, &order)?;

        // send collateral back to the user
        self.add_token_transfer_msg(ctx, &order.owner, order.collateral)?;

        ctx.response.add_event(CancelLimitOrderEvent { order_id });

        Ok(())
    }

    fn limit_order_remove(&self, storage: &mut dyn Storage, order: &LimitOrder) -> Result<()> {
        LIMIT_ORDERS.remove(storage, order.order_id);

        let market_type = self.market_type(storage)?;
        match order.direction {
            DirectionToNotional::Long => {
                LIMIT_ORDERS_BY_PRICE_LONG.remove(
                    storage,
                    (
                        order.trigger_price.into_price_key(market_type),
                        order.order_id,
                    ),
                );
            }
            DirectionToNotional::Short => {
                LIMIT_ORDERS_BY_PRICE_SHORT.remove(
                    storage,
                    (
                        order.trigger_price.into_price_key(market_type),
                        order.order_id,
                    ),
                );
            }
        }

        LIMIT_ORDERS_BY_ADDR.remove(storage, (&order.owner, order.order_id));

        Ok(())
    }

    pub(crate) fn limit_order_get_history(
        &self,
        store: &dyn Storage,
        addr: &Addr,
        start_after: Option<u64>,
        limit: Option<u32>,
        order: Option<Order>,
    ) -> Result<LimitOrderHistoryResp> {
        let (orders, next_start_after) = self.get_history_helper(
            EXECUTED_LIMIT_ORDERS,
            store,
            addr,
            start_after,
            limit,
            order,
        )?;

        Ok(LimitOrderHistoryResp {
            orders,
            next_start_after,
        })
    }
}
