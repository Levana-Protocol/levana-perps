use crate::state::*;
use cosmwasm_std::Event;

use perpswap::prelude::*;

impl State<'_> {
    pub(crate) fn force_withdraw_all(&self, ctx: &mut StateContext, limit: u32) -> Result<()> {
        let mut processed_lp = 0u32;
        let mut processed_limit_orders = 0u32;

        while processed_lp + processed_limit_orders < limit {
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

        while processed_lp + processed_limit_orders < limit {
            let lp = self
                .liquidity_providers(ctx.storage, None, 1)?
                .into_iter()
                .next();
            let lp = match lp {
                Some(lp) => lp,
                None => break,
            };

            let remaining = limit - processed_lp - processed_limit_orders - 1;
            let user_limit_orders = self.force_withdraw_user_funds(ctx, &lp, remaining)?;
            processed_lp += 1;
            processed_limit_orders += user_limit_orders;
        }

        let remaining_lp = !self.liquidity_providers(ctx.storage, None, 1)?.is_empty();
        let remaining_limit_orders = !self.limit_order_ids(ctx.storage, None, 1)?.is_empty();

        ctx.response_mut().add_event(
            Event::new("force-withdraw-all")
                .add_attribute("processed-lp", processed_lp.to_string())
                .add_attribute("processed-limit-orders", processed_limit_orders.to_string())
                .add_attribute("remaining-lp", remaining_lp.to_string())
                .add_attribute("remaining-limit-orders", remaining_limit_orders.to_string()),
        );

        Ok(())
    }
}
