use crate::state::{State, StateContext};
use anyhow::{Context, Result};
use cosmwasm_std::{Addr, Order, Storage};
use cw_storage_plus::{Bound, Item, Map, PrefixBound};
use msg::contracts::market::entry::{LimitOrderResp, LimitOrdersResp};
use msg::contracts::market::fees::events::TradeId;
use msg::contracts::market::order::events::{
    CancelLimitOrderEvent, ExecuteLimitOrderEvent, PlaceLimitOrderEvent,
};
use msg::contracts::market::order::{LimitOrder, OrderId};
use msg::prelude::*;

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
    ) -> Result<()> {
        let last_order_id = LAST_ORDER_ID
            .may_load(ctx.storage)?
            .unwrap_or(OrderId(0u64));
        let order_id = OrderId(last_order_id.u64() + 1);
        LAST_ORDER_ID.save(ctx.storage, &order_id)?;

        let order_fee = Collateral::try_from_number(
            collateral
                .into_number()
                .checked_mul(self.config.limit_order_fee.into_number())?,
        )?;
        let collateral = collateral.checked_sub(order_fee)?;
        let price = self.spot_price(ctx.storage, None)?;
        self.collect_limit_order_fee(ctx, order_id, order_fee, price)?;

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
        };

        self.limit_order_validate(ctx.storage, &order)?;

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

        Ok(())
    }

    /// Returns the next long or short [LimitOrder] whose trigger price is above the specified price
    /// for long orders or below the specified price for short orders.
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
                Order::Ascending,
            )
            .next();

        let order = match order {
            Some(_) => order,
            None => LIMIT_ORDERS_BY_PRICE_SHORT
                .prefix_range(
                    storage,
                    None,
                    Some(PrefixBound::inclusive(PriceKey::from(price))),
                    Order::Descending,
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

    /// Attempts to execute the specified limit order by opening a position.
    /// If the position fails to open, the limit order is removed from the protocol.
    pub(crate) fn limit_order_execute_order(
        &self,
        ctx: &mut StateContext,
        order_id: OrderId,
    ) -> Result<()> {
        let order = LIMIT_ORDERS.load(ctx.storage, order_id)?;
        self.limit_order_remove(ctx.storage, &order)?;

        let market_type = self.market_type(ctx.storage)?;

        //FIXME do something with the error so the user knows what's going on
        let res = self.handle_position_open(
            ctx,
            order.owner,
            order.collateral,
            order.leverage,
            order.direction.into_base(market_type),
            order.max_gains,
            None,
            order.stop_loss_override,
            order.take_profit_override,
        );

        ctx.response.add_event(ExecuteLimitOrderEvent {
            order_id,
            pos_id: match res {
                Ok(pos_id) => Some(pos_id),
                Err(_) => None,
            },
        });

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

    fn limit_order_validate(&self, storage: &dyn Storage, order: &LimitOrder) -> Result<()> {
        let price = self.spot_price(storage, None)?;
        let market_type = self.market_type(storage)?;

        match order.direction {
            DirectionToNotional::Long => {
                self.validate_order_price(
                    order.trigger_price.into_notional_price(market_type),
                    order.trigger_price,
                    order
                        .stop_loss_override
                        .map(|price| price.into_notional_price(market_type)),
                    order.stop_loss_override,
                    Some(price.price_notional),
                    Some(price.price_base),
                    market_type,
                    "Limit order",
                )?;
            }
            DirectionToNotional::Short => {
                self.validate_order_price(
                    order.trigger_price.into_notional_price(market_type),
                    order.trigger_price,
                    Some(price.price_notional),
                    Some(price.price_base),
                    order
                        .stop_loss_override
                        .map(|price| price.into_notional_price(market_type)),
                    order.stop_loss_override,
                    market_type,
                    "Limit order",
                )?;
            }
        }

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
}
