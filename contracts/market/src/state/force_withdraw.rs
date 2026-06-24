use crate::state::*;
use cosmwasm_std::{Event, Order};

use perpswap::prelude::*;

use super::position::{close::ClosePositionExec, OPEN_POSITIONS};

impl State<'_> {
    pub(crate) fn force_withdraw_all(&self, ctx: &mut StateContext, limit: u32) -> Result<()> {
        let mut processed_positions = 0u32;
        let mut processed_lp = 0u32;
        let mut processed_limit_orders = 0u32;

        // Phase 0: force-close all open positions to free locked collateral.
        // This moves counterparty collateral from locked → unlocked so that
        // LP withdrawals can proceed.
        let settlement_price = self.current_spot_price(ctx.storage)?;
        while processed_positions + processed_lp + processed_limit_orders < limit {
            let pos_entry = OPEN_POSITIONS
                .range(ctx.storage, None, None, Order::Ascending)
                .next();
            let (_, pos) = match pos_entry {
                Some(Ok(entry)) => entry,
                Some(Err(e)) => return Err(e.into()),
                None => break,
            };

            ClosePositionExec::new_via_msg(self, ctx.storage, pos, settlement_price)?
                .apply(self, ctx)?;
            processed_positions += 1;
        }

        // Phase 1: cancel orphaned limit orders
        while processed_positions + processed_lp + processed_limit_orders < limit {
            let order_id = self
                .limit_order_ids(ctx.storage, None, 1)?
                .into_iter()
                .next();
            let order_id = match order_id {
                Some(order_id) => order_id,
                None => break,
            };

            self.force_cancel_limit_order(ctx, order_id)?;
            processed_limit_orders += 1;
        }

        // Phase 2: force-withdraw LPs
        while processed_positions + processed_lp + processed_limit_orders < limit {
            let lp = self
                .liquidity_providers(ctx.storage, None, 1)?
                .into_iter()
                .next();
            let lp = match lp {
                Some(lp) => lp,
                None => break,
            };

            let remaining = limit
                .saturating_sub(processed_positions)
                .saturating_sub(processed_lp)
                .saturating_sub(processed_limit_orders)
                .saturating_sub(1);
            let user_limit_orders = self.force_withdraw_user_funds(ctx, &lp, remaining)?;
            processed_lp += 1;
            processed_limit_orders += user_limit_orders;
        }

        let remaining_positions = OPEN_POSITIONS
            .range(ctx.storage, None, None, Order::Ascending)
            .next()
            .is_some();
        let remaining_lp = !self.liquidity_providers(ctx.storage, None, 1)?.is_empty();
        let remaining_limit_orders = !self.limit_order_ids(ctx.storage, None, 1)?.is_empty();

        ctx.response_mut().add_event(
            Event::new("force-withdraw-all")
                .add_attribute("processed-positions", processed_positions.to_string())
                .add_attribute("processed-lp", processed_lp.to_string())
                .add_attribute("processed-limit-orders", processed_limit_orders.to_string())
                .add_attribute("remaining-positions", remaining_positions.to_string())
                .add_attribute("remaining-lp", remaining_lp.to_string())
                .add_attribute("remaining-limit-orders", remaining_limit_orders.to_string()),
        );

        Ok(())
    }
}
