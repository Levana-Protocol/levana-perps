use crate::state::{State, StateContext};
use anyhow::{Context, Result};
use cosmwasm_std::{Addr, Order, Storage};
use cw_storage_plus::{Bound, Item, Map, PrefixBound};
use perpswap::compat::BackwardsCompatTakeProfit;
use perpswap::contracts::market::entry::{
    ExecutedLimitOrder, LimitOrderHistoryResp, LimitOrderResp, LimitOrderResult, LimitOrdersResp,
};
use perpswap::contracts::market::fees::events::TradeId;
use perpswap::contracts::market::order::events::{
    CancelLimitOrderEvent, ExecuteLimitOrderEvent, PlaceLimitOrderEvent,
};
use perpswap::contracts::market::order::{LimitOrder, OrderId};
use perpswap::contracts::market::position::events::PositionSaveReason;
use perpswap::contracts::market::position::CollateralAndUsd;
use perpswap::prelude::*;

use super::fees::CapCrankFee;
use super::position::{OpenPositionExec, OpenPositionParams};

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

        #[allow(deprecated)]
        let take_profit_trader = match (order.take_profit, order.max_gains) {
            (None, None) => {
                bail!("must supply at least one of take_profit or max_gains");
            }
            (Some(take_profit_price), None) => take_profit_price,
            (take_profit, Some(max_gains)) => {
                let take_profit = match take_profit {
                    None => None,
                    Some(take_profit) => match take_profit {
                        TakeProfitTrader::PosInfinity => {
                            bail!("cannot set infinite take profit price and max_gains")
                        }
                        TakeProfitTrader::Finite(x) => Some(PriceBaseInQuote::from_non_zero(x)),
                    },
                };
                BackwardsCompatTakeProfit {
                    collateral: order.collateral,
                    market_type,
                    direction: order.direction.into_base(market_type),
                    leverage: order.leverage,
                    max_gains,
                    take_profit,
                    price_point,
                }
                .calc()?
            }
        };

        let open_position_params = OpenPositionParams {
            owner: order.owner.clone(),
            collateral: order.collateral,
            crank_fee: CollateralAndUsd::from_pair(order.crank_fee_collateral, order.crank_fee_usd),
            leverage: order.leverage,
            direction: order.direction.into_base(market_type),
            slippage_assert: None,
            stop_loss_override: order.stop_loss_override,
            take_profit_trader,
        };

        let res = match OpenPositionExec::new(self, ctx.storage, open_position_params, price_point)
        {
            Ok(validated_position) => {
                let pos_id =
                    validated_position.apply(self, ctx, PositionSaveReason::ExecuteLimitOrder)?;
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

        const ORDER_DEFAULT_LIMIT: u32 = 20;

        let limit = limit
            .unwrap_or(ORDER_DEFAULT_LIMIT)
            .min(QUERY_MAX_LIMIT)
            .try_into()?;
        let mut orders = Vec::with_capacity(limit);
        let mut next_start_after = None;
        for _ in 0..limit {
            match iter.next().transpose()? {
                Some((order_id, _)) => {
                    let order = LIMIT_ORDERS.load(storage, order_id)?;
                    let market_type = self.market_type(storage)?;

                    #[allow(deprecated)]
                    let order_resp = LimitOrderResp {
                        order_id,
                        trigger_price: order.trigger_price,
                        collateral: order.collateral,
                        leverage: order.leverage,
                        direction: order.direction.into_base(market_type),
                        max_gains: order.max_gains,
                        stop_loss_override: order.stop_loss_override,
                        take_profit: backwards_compat_limit_order_take_profit(
                            self, storage, &order,
                        )?,
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

#[must_use]
pub(crate) struct PlaceLimitOrderExec {
    order_id: OrderId,
    crank_fee: CapCrankFee,
    order: LimitOrder,
    price: PricePoint,
}

impl PlaceLimitOrderExec {
    /// Sets a [LimitOrder]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        owner: Addr,
        trigger_price: PriceBaseInQuote,
        collateral: NonZero<Collateral>,
        leverage: LeverageToBase,
        direction: DirectionToNotional,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit: TakeProfitTrader,
        deferred_exec_crank_fee: Collateral,
        deferred_exec_crank_fee_usd: Usd,
        price: PricePoint,
    ) -> Result<Self> {
        let last_order_id = LAST_ORDER_ID
            .may_load(store)?
            .unwrap_or_else(|| OrderId::new(0));
        let order_id = OrderId::new(last_order_id.u64() + 1);

        let crank_fee = CapCrankFee::new(
            price.usd_to_collateral(state.config.crank_fee_charged),
            state.config.crank_fee_charged,
            TradeId::LimitOrder(order_id),
        );
        let collateral = collateral
            .checked_sub(crank_fee.amount)
            .context("Insufficient funds to cover fees, failed on crank fee")?;

        #[allow(deprecated)]
        let order = LimitOrder {
            order_id,
            owner: owner.clone(),
            trigger_price,
            collateral,
            leverage,
            direction,
            stop_loss_override,
            max_gains: None,
            take_profit: Some(take_profit),
            crank_fee_collateral: crank_fee.amount.checked_add(deferred_exec_crank_fee)?,
            crank_fee_usd: crank_fee
                .amount_usd
                .checked_add(deferred_exec_crank_fee_usd)?,
        };

        Ok(Self {
            order_id,
            crank_fee,
            order,
            price,
        })
    }

    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}

    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<OrderId> {
        let Self {
            order_id,
            crank_fee,
            order,
            price,
        } = self;

        LAST_ORDER_ID.save(ctx.storage, &order_id)?;
        crank_fee.apply(state, ctx)?;

        LIMIT_ORDERS.save(ctx.storage, order_id, &order)?;

        let market_type = state.market_type(ctx.storage)?;

        match order.direction {
            DirectionToNotional::Long => LIMIT_ORDERS_BY_PRICE_LONG.save(
                ctx.storage,
                (order.trigger_price.into_price_key(market_type), order_id),
                &(),
            )?,
            DirectionToNotional::Short => LIMIT_ORDERS_BY_PRICE_SHORT.save(
                ctx.storage,
                (order.trigger_price.into_price_key(market_type), order_id),
                &(),
            )?,
        }

        LIMIT_ORDERS_BY_ADDR.save(ctx.storage, (&order.owner, order_id), &())?;

        let direction_to_base = order.direction.into_base(market_type);

        #[allow(deprecated)]
        ctx.response.add_event(PlaceLimitOrderEvent {
            market_type,
            collateral: order.collateral,
            collateral_usd: price.collateral_to_usd_non_zero(order.collateral),
            leverage: order.leverage.into_signed(direction_to_base),
            direction: direction_to_base,
            max_gains: order.max_gains,
            stop_loss_override: order.stop_loss_override,
            order_id,
            owner: order.owner,
            trigger_price: order.trigger_price,
            take_profit_override: order.take_profit,
        });
        Ok(self.order_id)
    }
}

#[must_use]
pub(crate) struct CancelLimitOrderExec {
    order_id: OrderId,
    order: LimitOrder,
}

impl CancelLimitOrderExec {
    /// Cancels a [LimitOrder]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(store: &dyn Storage, order_id: OrderId) -> Result<Self> {
        let order = LIMIT_ORDERS.load(store, order_id)?;

        Ok(Self { order_id, order })
    }

    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}

    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<()> {
        let Self { order_id, order } = self;
        state.limit_order_remove(ctx.storage, &order)?;

        // send collateral back to the user
        state.add_token_transfer_msg(ctx, &order.owner, order.collateral)?;

        ctx.response.add_event(CancelLimitOrderEvent { order_id });

        Ok(())
    }
}

// this will eventually be removed, it's just for backwards-compat
pub(crate) fn backwards_compat_limit_order_take_profit(
    state: &State,
    store: &dyn Storage,
    order: &LimitOrder,
) -> Result<TakeProfitTrader> {
    match order.take_profit {
        Some(x) => Ok(x),
        None => {
            let market_type = state.market_type(store)?;

            // we want to use the trigger price here, but, we need to get it as a PricePoint
            // this isn't done anywhere else in the codebase, so just create one via patching here
            let mut price_point = state.current_spot_price(store)?;

            price_point.price_notional = order.trigger_price.into_notional_price(market_type);
            price_point.price_base = order.trigger_price;
            price_point.price_usd = PriceCollateralInUsd::from_non_zero(
                NonZero::new(
                    price_point
                        .base_to_usd(Base::from_decimal256(
                            order.trigger_price.into_non_zero().into_decimal256(),
                        ))
                        .into_decimal256(),
                )
                .context("must be non-zero")?,
            );
            #[allow(deprecated)]
            BackwardsCompatTakeProfit {
                collateral: order.collateral,
                leverage: order.leverage,
                direction: order.direction.into_base(market_type),
                max_gains: order
                    .max_gains
                    .context("max_gains should be set in limit order backwards-compat branch")?,
                take_profit: None,
                market_type,
                price_point: &price_point,
            }
            .calc()
        }
    }
}
