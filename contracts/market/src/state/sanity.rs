//! Responsible for sanity checks that are tested during off-chain tests.

use std::collections::HashMap;

use crate::prelude::*;
use crate::state::{
    config::load_config,
    fees::ALL_FEES,
    funding::get_total_funding_margin,
    position::{NEXT_LIQUIFUNDING, OPEN_POSITIONS, PRICE_TRIGGER_ASC, PRICE_TRIGGER_DESC},
    token::TOKEN,
};
use cosmwasm_std::{Env, Order, QuerierWrapper};
use msg::contracts::market::entry::Fees;
use msg::contracts::market::{
    entry::{LpInfoResp, UnstakingStatus},
    liquidity::LiquidityStats,
    position::{LiquidationReason, PositionId},
};

use super::{
    delta_neutrality_fee::DELTA_NEUTRALITY_FUND, funding::get_total_net_funding_paid, State,
};

impl State<'_> {
    // the ubiquitous check inserted at the top of every entry point
    pub(crate) fn sanity_check(&self, store: &dyn Storage) {
        next_last_liquifunding(store, &self.env)
            .expect("next_last_liquifunding sanity check failed");
        liquidation_prices(store, &self.env).expect("liquidation_prices sanity check failed");
        locked_balances(self, store).expect("locked balance does not match counter collateral");
        liquidity_balances(self, store, &self.env, &self.querier)
            .expect("liquidity_balances sanity check failed");
        sufficient_collateral_for_margins(store).expect("insufficient collateral for margins");
        valid_lp_info(self, store).expect("Invalid lp_info response found");
    }
}

// we need to check token balance *after* the funds have been processed
// so this is added at the end of the execute handler
pub(crate) fn sanity_check_post_execute(
    state: &State,
    store: &dyn Storage,
    env: &Env,
    querier: &QuerierWrapper,
    fund_transfers: &HashMap<Addr, NonZero<Collateral>>,
) {
    state.sanity_check(store);
    token_balance(state, store, env, querier, fund_transfers).expect("token_balance check failed");
}

fn next_last_liquifunding(store: &dyn Storage, env: &Env) -> Result<()> {
    let config = load_config(store)?;
    let delay = Duration::from_seconds(config.liquifunding_delay_seconds.into());
    let open_position_count = OPEN_POSITIONS
        .keys(store, None, None, cosmwasm_std::Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?
        .len();
    let now = Timestamp::from(env.block.time);

    let mut next_count = 0;
    for pair in NEXT_LIQUIFUNDING.keys(store, None, None, cosmwasm_std::Order::Ascending) {
        let (timestamp, position_id) = pair?;
        let position = OPEN_POSITIONS.load(store, position_id)?;
        anyhow::ensure!(
            timestamp < now || (timestamp.checked_sub(now, "next_last_liquifunding (1)")?) <= delay
        );
        // Thanks to randomization, this can happen early
        anyhow::ensure!(position.liquifunded_at + delay >= timestamp);
        next_count += 1;
    }
    anyhow::ensure!(next_count == open_position_count);

    for res in OPEN_POSITIONS.range(store, None, None, cosmwasm_std::Order::Ascending) {
        let (position_id, pos) = res?;
        anyhow::ensure!(NEXT_LIQUIFUNDING.has(store, (pos.next_liquifunding, position_id)));
    }

    Ok(())
}

fn liquidation_prices(store: &dyn Storage, _env: &Env) -> Result<()> {
    for res in OPEN_POSITIONS.range(store, None, None, cosmwasm_std::Order::Ascending) {
        let (posid, pos) = res?;

        let liquidation = pos.liquidation_price;
        let take_profit_total = pos.take_profit_total;
        let stop_loss_override = pos.stop_loss_override_notional;
        let take_profit_trader_notional = pos.take_profit_trader_notional;
        match pos.direction() {
            DirectionToNotional::Long => {
                match liquidation {
                    Some(price) => {
                        PRICE_TRIGGER_DESC.load(store, (price.into(), posid))?;
                    }
                    None => {
                        ensure_missing(store, PRICE_TRIGGER_DESC, posid)?;
                    }
                }
                match take_profit_total {
                    Some(price) => {
                        PRICE_TRIGGER_ASC.load(store, (price.into(), posid))?;
                    }
                    None => {
                        ensure_missing(store, PRICE_TRIGGER_ASC, posid)?;
                    }
                }
                match stop_loss_override {
                    Some(price) => {
                        PRICE_TRIGGER_DESC.load(store, (price.into(), posid))?;
                    }
                    None => {
                        if liquidation.is_none() {
                            ensure_missing(store, PRICE_TRIGGER_DESC, posid)?;
                        } else {
                            ensure_at_most_one(store, PRICE_TRIGGER_DESC, posid)?;
                        }
                    }
                }
                match take_profit_trader_notional {
                    Some(price) => {
                        PRICE_TRIGGER_ASC.load(store, (price.into(), posid))?;
                    }
                    None => {
                        if take_profit_total.is_none() {
                            ensure_missing(store, PRICE_TRIGGER_ASC, posid)?;
                        } else {
                            ensure_at_most_one(store, PRICE_TRIGGER_ASC, posid)?;
                        }
                    }
                }
            }
            DirectionToNotional::Short => {
                match liquidation {
                    Some(price) => {
                        PRICE_TRIGGER_ASC.load(store, (price.into(), posid))?;
                    }
                    None => {
                        ensure_missing(store, PRICE_TRIGGER_ASC, posid)?;
                    }
                }
                match take_profit_total {
                    Some(price) => {
                        PRICE_TRIGGER_DESC.load(store, (price.into(), posid))?;
                    }
                    None => {
                        ensure_missing(store, PRICE_TRIGGER_DESC, posid)?;
                    }
                }
                match stop_loss_override {
                    Some(price) => {
                        PRICE_TRIGGER_ASC.load(store, (price.into(), posid))?;
                    }
                    None => {
                        if liquidation.is_none() {
                            ensure_missing(store, PRICE_TRIGGER_ASC, posid)?;
                        } else {
                            ensure_at_most_one(store, PRICE_TRIGGER_ASC, posid)?;
                        }
                    }
                }
                match take_profit_trader_notional {
                    Some(price) => {
                        PRICE_TRIGGER_DESC.load(store, (price.into(), posid))?;
                    }
                    None => {
                        if take_profit_total.is_none() {
                            ensure_missing(store, PRICE_TRIGGER_DESC, posid)?;
                        } else {
                            ensure_at_most_one(store, PRICE_TRIGGER_DESC, posid)?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn locked_balances(state: &State, store: &dyn Storage) -> Result<()> {
    let stats = state.load_liquidity_stats(store)?;
    let mut counter_collateral = Collateral::zero();
    for position in OPEN_POSITIONS.range(store, None, None, cosmwasm_std::Order::Ascending) {
        let (_, position) = position?;
        counter_collateral = (counter_collateral + position.counter_collateral.raw())?;
    }

    anyhow::ensure!(stats.locked == counter_collateral);

    Ok(())
}

/// Ensure that the given map does not contain the given position ID.
fn ensure_missing(
    store: &dyn Storage,
    m: Map<(PriceKey, PositionId), LiquidationReason>,
    posid: PositionId,
) -> Result<()> {
    for res in m.keys(store, None, None, cosmwasm_std::Order::Ascending) {
        let (_, x) = res?;
        anyhow::ensure!(
            x != posid,
            "found entry for position {} in liquidation map, but it shouldn't be in there",
            posid
        );
    }
    Ok(())
}

fn ensure_at_most_one(
    store: &dyn Storage,
    m: Map<(PriceKey, PositionId), LiquidationReason>,
    posid: PositionId,
) -> Result<()> {
    let mut found_one = false;

    for res in m.keys(store, None, None, cosmwasm_std::Order::Ascending) {
        let (_, x) = res?;
        // anyhow::ensure!(x != posid);
        if x == posid {
            if found_one {
                anyhow::bail!("found more than one entry for {}", posid);
            } else {
                found_one = true;
            }
        }
    }

    Ok(())
}

fn liquidity_balances(
    state: &State,
    store: &dyn Storage,
    env: &Env,
    querier: &QuerierWrapper,
) -> Result<()> {
    // If we're in the middle of resetting, just exit, the balances are known to
    // be broken right now.
    if state.should_reset_lp_balances(store)? {
        return Ok(());
    }

    let LiquidityStats {
        locked,
        unlocked,
        total_lp,
        total_xlp,
    } = state.load_liquidity_stats(store)?;

    let mut sum_lp = LpToken::zero();
    let mut sum_xlp = LpToken::zero();
    for res in state.iter_liquidity_stats_addrs(store) {
        let (_, value) = res?;
        anyhow::ensure!(!value.is_empty());
        sum_lp = (sum_lp + value.lp)?;
        sum_xlp = (sum_xlp + value.xlp)?;
        if let Some(unstaking) = value.unstaking {
            sum_lp = (sum_lp + (unstaking.xlp_amount.raw() - unstaking.collected)?)?;
        }
    }
    anyhow::ensure!(total_lp == sum_lp);
    anyhow::ensure!(total_xlp == sum_xlp);

    let token = TOKEN.load(store)?;
    let actual = token.query_balance(querier, &env.contract.address)?;

    // exact amounts are checked in token_balance
    anyhow::ensure!((locked + unlocked)? <= actual);
    Ok(())
}

#[derive(Debug)]
struct SubTotals {
    position_collateral: Collateral,
    limit_order_collateral: Collateral,
    pending_transfer: Collateral,
    liquidity_stats: LiquidityStats,
    fees: Fees,
    net_funding_paid: Signed<Collateral>,
    delta_neutrality_fund: Collateral,
    deferred_exec: Collateral,
}

impl SubTotals {
    fn load(
        state: &State,
        store: &dyn Storage,
        fund_transfers: &HashMap<Addr, NonZero<Collateral>>,
    ) -> Result<Self> {
        let fees = ALL_FEES.may_load(store)?.context("ALL_FEES not set")?;
        let mut pending_transfer = Collateral::zero();

        for transfer in fund_transfers.values() {
            pending_transfer = (pending_transfer + transfer.raw())?;
        }

        let mut subtotals = SubTotals {
            position_collateral: Collateral::zero(),
            limit_order_collateral: Collateral::zero(),
            liquidity_stats: state.load_liquidity_stats(store)?,
            fees,
            pending_transfer,
            net_funding_paid: get_total_net_funding_paid(store)?,
            delta_neutrality_fund: DELTA_NEUTRALITY_FUND.may_load(store)?.unwrap_or_default(),
            deferred_exec: state.deferred_exec_deposit_balance(store)?,
        };

        for position in OPEN_POSITIONS.range(store, None, None, Order::Ascending) {
            let (_, position) = position?;
            subtotals.position_collateral =
                (subtotals.position_collateral + position.active_collateral.raw())?;
        }

        for order in state.limit_order_load_all(store)? {
            subtotals.limit_order_collateral =
                (subtotals.limit_order_collateral + order.collateral.raw())?;
        }

        Ok(subtotals)
    }

    fn total(&self) -> Result<Collateral> {
        let SubTotals {
            position_collateral,
            limit_order_collateral,
            pending_transfer,
            liquidity_stats,
            fees,
            net_funding_paid: funding_fee_imbalance,
            delta_neutrality_fund,
            deferred_exec,
        } = self;

        let mut positive_total = Collateral::zero();

        for value in [
            *pending_transfer,
            *position_collateral,
            *limit_order_collateral,
            liquidity_stats.unlocked,
            liquidity_stats.locked,
            fees.protocol,
            fees.wallets,
            fees.crank,
            *delta_neutrality_fund,
            *deferred_exec,
        ] {
            positive_total = (positive_total + value)?;
        }

        let total = (positive_total.into_signed() + *funding_fee_imbalance)?;

        total
            .try_into_non_negative_value()
            .with_context(|| format!("Calculated total is negative: {self:?}"))
    }
}

fn token_balance(
    state: &State,
    store: &dyn Storage,
    env: &Env,
    querier: &QuerierWrapper,
    fund_transfers: &HashMap<Addr, NonZero<Collateral>>,
) -> Result<()> {
    let subtotals = SubTotals::load(state, store, fund_transfers)?;
    debug_log!(
        DebugLog::SanityFundsSubtotal,
        "[sanity funds] {:#?}",
        subtotals
    );

    let calculated_total = subtotals.total()?;

    // check that it all adds up
    let token = TOKEN.load(store)?;
    let token_balance = token.query_balance(querier, &env.contract.address)?;

    debug_log!(
        DebugLog::SanityFundsBalanceAssertion,
        "[sanity funds] asserting that token balance {token_balance} == {calculated_total}"
    );

    // Check equality up to 4 decimal points. CW20s support 6 decimals of
    // precision, so there are known errors that can occur versus our 18 digits
    // of precision.
    let epsilon = "0.0001".parse()?;
    anyhow::ensure!(
        (token_balance.into_signed() - calculated_total.into_signed())?.abs() < epsilon,
        "Mismatched token balance.  Actual: {token_balance}. Calculated: {calculated_total}.\nDetails: {subtotals:?}"
    );

    Ok(())
}

fn sufficient_collateral_for_margins(store: &dyn Storage) -> Result<()> {
    let mut total_funding_margin_calculated = Collateral::zero();
    for res in OPEN_POSITIONS.range(store, None, None, Order::Ascending) {
        let (_, pos) = res?;
        anyhow::ensure!(pos.active_collateral.raw() >= pos.liquidation_margin.total()?);
        total_funding_margin_calculated =
            (total_funding_margin_calculated + pos.liquidation_margin.funding)?;
    }

    anyhow::ensure!(total_funding_margin_calculated == get_total_funding_margin(store)?);
    Ok(())
}

fn valid_lp_info(state: &State, store: &dyn Storage) -> Result<()> {
    for res in state.iter_liquidity_stats_addrs(store) {
        let lp_addr = res?.0;
        let lp_info = state.lp_info(store, &lp_addr)?;
        check_lp_info(&lp_info)
            .with_context(|| format!("Invalid lp_info for {lp_addr}: {lp_info:?}"))?;
    }
    Ok(())
}

fn check_lp_info(
    LpInfoResp {
        lp_amount,
        lp_collateral,
        xlp_amount,
        xlp_collateral,
        available_yield,
        available_yield_lp,
        available_yield_xlp,
        available_crank_rewards,
        available_referrer_rewards,
        unstaking,
        history: _,
        liquidity_cooldown: _,
    }: &LpInfoResp,
) -> Result<()> {
    anyhow::ensure!(lp_amount.is_zero() == lp_collateral.is_zero());
    anyhow::ensure!(xlp_amount.is_zero() == xlp_collateral.is_zero());
    anyhow::ensure!(
        *available_yield
            == (((*available_yield_lp + *available_yield_xlp)? + *available_crank_rewards)?
                + *available_referrer_rewards)?
    );
    if let Some(unstaking) = unstaking {
        let UnstakingStatus {
            start: _,
            end: _,
            xlp_unstaking,
            xlp_unstaking_collateral: _,
            collected,
            available,
            pending,
        } = unstaking;
        anyhow::ensure!(xlp_unstaking.raw() == ((*collected + *available)? + *pending)?);
    }
    Ok(())
}
