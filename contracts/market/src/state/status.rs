use crate::prelude::*;
use anyhow::Result;
use cosmwasm_std::Storage;
use msg::contracts::market::entry::StatusResp;

use super::{crank::LAST_CRANK_COMPLETED, fees::all_fees, State};
use crate::state::delta_neutrality_fee::DELTA_NEUTRALITY_FUND;

impl State<'_> {
    pub(crate) fn status(&self, store: &dyn Storage) -> Result<StatusResp> {
        let market_id = self.market_id(store)?;
        let market_type = market_id.get_market_type();

        let collateral = self.get_token(store)?.clone();
        let next_crank_timestamp = self.next_crank_timestamp(store)?;
        let next_crank = match next_crank_timestamp {
            None => None,
            Some(price_point) => Some(self.crank_work(store, price_point)?),
        };

        let liquidity = self.load_liquidity_stats(store)?;

        let (long_funding_notional, short_funding_notional) =
            self.derive_instant_funding_rate_annual(store)?;
        let borrow_fee = self.get_current_borrow_fee_rate_annual(store)?.1;

        let (long_funding, short_funding) = match market_type {
            MarketType::CollateralIsQuote => (long_funding_notional, short_funding_notional),
            MarketType::CollateralIsBase => (short_funding_notional, long_funding_notional),
        };

        // Use the market type internal to the protocol
        let long_notional_protocol = self.open_long_interest(store)?;
        let short_notional_protocol = self.open_short_interest(store)?;

        // Switch to match direction-in-base
        let (long_notional, short_notional) = match market_type {
            MarketType::CollateralIsQuote => (long_notional_protocol, short_notional_protocol),
            MarketType::CollateralIsBase => (short_notional_protocol, long_notional_protocol),
        };

        // Avoid spot price lookup for corner case of status queries before spot price update
        let (long_usd, short_usd) = if long_notional.is_zero() && short_notional.is_zero() {
            (Usd::zero(), Usd::zero())
        } else {
            let price_point = self.current_spot_price(store)?;
            let long_usd = price_point.notional_to_usd(long_notional);
            let short_usd = price_point.notional_to_usd(short_notional);
            (long_usd, short_usd)
        };
        let instant_delta_neutrality_fee_value = long_notional
            .into_signed()
            .checked_sub(short_notional.into_signed())?
            .into_number()
            .checked_div(self.config.delta_neutrality_fee_sensitivity.into_signed())?;
        let delta_neutrality_fee_fund = DELTA_NEUTRALITY_FUND
            .may_load(store)?
            .unwrap_or(Collateral::zero());

        let fees = all_fees(store)?;

        let last_crank_completed = LAST_CRANK_COMPLETED.may_load(store)?;

        let next_deferred_execution = self
            .get_next_deferred_execution(store)?
            .map(|(_, item)| item.created);
        let deferred_execution_items = self.deferred_execution_items(store)?;

        Ok(StatusResp {
            market_id: market_id.clone(),
            base: market_id.get_base().to_owned(),
            quote: market_id.get_quote().to_owned(),
            market_type,
            collateral,
            config: self.config.clone(),
            liquidity,
            next_crank,
            borrow_fee: borrow_fee.total(),
            borrow_fee_lp: borrow_fee.lp,
            borrow_fee_xlp: borrow_fee.xlp,
            long_funding,
            short_funding,
            long_notional,
            short_notional,
            long_usd,
            short_usd,
            instant_delta_neutrality_fee_value,
            delta_neutrality_fee_fund,
            fees,
            last_crank_completed,
            next_deferred_execution,
            deferred_execution_items,
        })
    }
}
