use crate::prelude::*;
use cosmwasm_std::Order;
use msg::contracts::market::{
    entry::{LpAction, LpActionHistoryResp, LpActionKind, LpHistorySummary},
    history::events::{LpActionEvent, LpDepositEvent, LpYieldEvent},
};
use shared::storage::push_to_monotonic_multilevel_map;

const LP_HISTORY_SUMMARY: Map<&Addr, LpHistorySummary> = Map::new(namespace::LP_HISTORY_SUMMARY);
const LP_HISTORY_BY_ADDRESS: Map<(&Addr, u64), LpAction> =
    Map::new(namespace::LP_HISTORY_BY_ADDRESS);

impl State<'_> {
    //******** MUTABLE API / SETTERS *****************//
    pub(crate) fn lp_history_add_deposit(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        tokens: NonZero<LpToken>,
        collateral: NonZero<Collateral>,
        is_xlp: bool,
    ) -> Result<()> {
        self.lp_history_add_action(
            ctx,
            addr,
            if is_xlp {
                LpActionKind::DepositXlp
            } else {
                LpActionKind::DepositLp
            },
            Some(tokens),
            collateral,
        )?;
        self.lp_history_add_summary_deposit(ctx, addr, collateral)?;

        Ok(())
    }

    pub(crate) fn lp_history_add_reinvest_yield(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        tokens: NonZero<LpToken>,
        collateral: NonZero<Collateral>,
        is_xlp: bool,
    ) -> Result<()> {
        self.lp_history_add_action(
            ctx,
            addr,
            if is_xlp {
                LpActionKind::ReinvestYieldXlp
            } else {
                LpActionKind::ReinvestYieldLp
            },
            Some(tokens),
            collateral,
        )?;

        Ok(())
    }

    pub(crate) fn lp_history_add_unstake_xlp(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        tokens: NonZero<LpToken>,
        collateral: NonZero<Collateral>,
    ) -> Result<()> {
        self.lp_history_add_action(
            ctx,
            addr,
            LpActionKind::UnstakeXlp,
            Some(tokens),
            collateral,
        )?;

        Ok(())
    }

    pub(crate) fn lp_history_add_withdraw(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        tokens: NonZero<LpToken>,
        collateral: NonZero<Collateral>,
    ) -> Result<()> {
        self.lp_history_add_action(ctx, addr, LpActionKind::Withdraw, Some(tokens), collateral)?;

        Ok(())
    }

    pub(crate) fn lp_history_add_claim_yield(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        collateral: NonZero<Collateral>,
    ) -> Result<()> {
        self.lp_history_add_action(ctx, addr, LpActionKind::ClaimYield, None, collateral)?;

        self.lp_history_add_summary_yield(ctx, addr, collateral)?;
        Ok(())
    }

    pub(crate) fn lp_history_add_collect_lp(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        tokens: NonZero<LpToken>,
        collateral: NonZero<Collateral>,
    ) -> Result<()> {
        self.lp_history_add_action(ctx, addr, LpActionKind::CollectLp, Some(tokens), collateral)?;
        Ok(())
    }

    //******** IMMUTABLE API / GETTERS *****************//
    pub(crate) fn lp_history_get_summary(
        &self,
        store: &dyn Storage,
        addr: &Addr,
    ) -> Result<LpHistorySummary> {
        Ok(LP_HISTORY_SUMMARY
            .may_load(store, addr)?
            .unwrap_or_default())
    }

    pub(crate) fn lp_action_get_history(
        &self,
        store: &dyn Storage,
        addr: &Addr,
        start_after: Option<u64>,
        limit: Option<u32>,
        order: Option<Order>,
    ) -> Result<LpActionHistoryResp> {
        let (actions, next_start_after) = self.get_history_helper(
            LP_HISTORY_BY_ADDRESS,
            store,
            addr,
            start_after,
            limit,
            order,
        )?;

        Ok(LpActionHistoryResp {
            actions,
            next_start_after,
        })
    }

    //******** LOCAL HELPERS *****************//
    fn lp_history_add_action(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        kind: LpActionKind,
        tokens: Option<NonZero<LpToken>>,
        collateral: NonZero<Collateral>,
    ) -> Result<u64> {
        let price = self.current_spot_price(ctx.storage)?;

        let collateral = collateral.raw();
        let collateral_usd = price.collateral_to_usd(collateral);

        let action = LpAction {
            kind,
            timestamp: self.now(),
            tokens: tokens.map(NonZero::raw),
            collateral,
            collateral_usd,
        };

        let action_id =
            push_to_monotonic_multilevel_map(ctx.storage, LP_HISTORY_BY_ADDRESS, addr, &action)?;

        ctx.response.add_event(LpActionEvent {
            addr: addr.clone(),
            action,
            action_id,
        });

        Ok(action_id)
    }

    fn lp_history_add_summary_deposit(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        deposit: NonZero<Collateral>,
    ) -> Result<()> {
        let mut summary = self.lp_history_get_summary(ctx.storage, addr)?;
        let price = self.current_spot_price(ctx.storage)?;

        let deposit = deposit.raw();
        let deposit_usd = price.collateral_to_usd(deposit);

        summary.deposit = (summary.deposit + deposit)?;
        summary.deposit_usd = (summary.deposit_usd + deposit_usd)?;

        LP_HISTORY_SUMMARY.save(ctx.storage, addr, &summary)?;

        ctx.response.add_event(LpDepositEvent {
            deposit,
            deposit_usd,
        });

        Ok(())
    }

    fn lp_history_add_summary_yield(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        yield_earned: NonZero<Collateral>,
    ) -> Result<()> {
        let mut summary = self.lp_history_get_summary(ctx.storage, addr)?;
        let price = self.current_spot_price(ctx.storage)?;

        let yield_earned = yield_earned.raw();
        let yield_usd = price.collateral_to_usd(yield_earned);

        summary.r#yield = (summary.r#yield + yield_earned)?;
        summary.yield_usd = (summary.yield_usd + yield_usd)?;

        LP_HISTORY_SUMMARY.save(ctx.storage, addr, &summary)?;

        ctx.response.add_event(LpYieldEvent {
            addr: addr.clone(),
            r#yield: yield_earned,
            yield_usd,
        });

        Ok(())
    }

    pub(crate) fn lp_history_add_summary_referral(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        referral_earned: NonZero<Collateral>,
    ) -> Result<()> {
        let mut summary = self.lp_history_get_summary(ctx.storage, addr)?;
        let price = self.current_spot_price(ctx.storage)?;
        summary.referrer = summary.referrer.checked_add(referral_earned.raw())?;
        let referral_earned_usd = price.collateral_to_usd(referral_earned.raw());
        summary.referrer_usd = summary.referrer_usd.checked_add(referral_earned_usd)?;
        LP_HISTORY_SUMMARY.save(ctx.storage, addr, &summary)?;
        Ok(())
    }
}
