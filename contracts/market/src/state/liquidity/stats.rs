//! Separate module to ensure invariants are respected

use std::{cell::RefCell, collections::HashMap};

use crate::prelude::*;
use cosmwasm_std::Order;
use cw_storage_plus::Bound;
use msg::contracts::market::liquidity::LiquidityStats;

use crate::state::{State, StateContext};

use super::LiquidityStatsByAddr;

/// Liquidity stats such as locked and unlocked liquidity
const LIQUIDITY_STATS: Item<LiquidityStats> = Item::new(namespace::LIQUIDITY_STATS);
/// Stats for each individual liquidity provider
const LIQUIDITY_STATS_BY_ADDR: Map<&Addr, LiquidityStatsByAddr> =
    Map::new(namespace::LIQUIDITY_STATS_BY_ADDR);

/// Are we in the middle of resetting the LP values to 0?
///
/// This item will be _unset_ if we are not currently resetting. We could have
/// an extra enum variant in the [ResetLpStatus] to handle that case, but
/// there's not really a point to that.
const RESET_LP_STATUS: Item<ResetLpStatus> = Item::new(namespace::RESET_LP_STATUS);

/// Current status of resetting.
#[derive(serde::Serialize, serde::Deserialize)]
enum ResetLpStatus {
    /// We're just starting and haven't actually reset any balances yet.
    Begin,
    /// We last reset the given address, so next iteration should continue from
    /// after there (start_after).
    LastSaw(Addr),
}

impl ResetLpStatus {
    fn as_bound(&self) -> Option<Bound<&Addr>> {
        match self {
            ResetLpStatus::Begin => None,
            ResetLpStatus::LastSaw(addr) => Some(Bound::exclusive(addr)),
        }
    }
}

#[derive(Default)]
pub(crate) struct LiquidityCache {
    protocol: RefCell<Option<LiquidityStats>>,
    addr: RefCell<HashMap<Addr, Option<LiquidityStatsByAddr>>>,
}

pub(crate) fn liquidity_init(store: &mut dyn Storage) -> Result<()> {
    LIQUIDITY_STATS
        .save(
            store,
            &LiquidityStats {
                locked: Collateral::zero(),
                unlocked: Collateral::zero(),
                total_lp: LpToken::zero(),
                total_xlp: LpToken::zero(),
            },
        )
        .map_err(|e| e.into())
}

impl State<'_> {
    pub(crate) fn load_liquidity_stats(&self, store: &dyn Storage) -> Result<LiquidityStats> {
        if let Some(x) = &*self.liquidity_cache.protocol.borrow() {
            return Ok(x.clone());
        }

        let liquidity_stats = LIQUIDITY_STATS.load(store)?;
        *self.liquidity_cache.protocol.borrow_mut() = Some(liquidity_stats.clone());

        Ok(liquidity_stats)
    }

    pub(crate) fn save_liquidity_stats(
        &self,
        store: &mut dyn Storage,
        stats: &LiquidityStats,
    ) -> Result<()> {
        // If we have no more liquidity in the system, but we still have some
        // active LP or xLP tokens. Need to ask the crank to wipe them out.
        if stats.total_collateral()?.is_zero() && !stats.total_tokens()?.is_zero() {
            let stats = LiquidityStats {
                locked: Collateral::zero(),
                unlocked: Collateral::zero(),
                total_lp: LpToken::zero(),
                total_xlp: LpToken::zero(),
            };
            LIQUIDITY_STATS.save(store, &stats)?;
            *self.liquidity_cache.protocol.borrow_mut() = Some(stats);
            *self.liquidity_cache.addr.borrow_mut() = HashMap::new();
            RESET_LP_STATUS.save(store, &ResetLpStatus::Begin)?;
            Ok(())
        } else {
            *self.liquidity_cache.protocol.borrow_mut() = Some(stats.clone());
            LIQUIDITY_STATS.save(store, stats).map_err(|e| e.into())
        }
    }

    /// Provides a default value
    pub(crate) fn load_liquidity_stats_addr_default(
        &self,
        store: &dyn Storage,
        lp_addr: &Addr,
    ) -> Result<LiquidityStatsByAddr> {
        Ok(match self.load_liquidity_stats_addr_may(store, lp_addr)? {
            Some(stats) => stats,
            None => LiquidityStatsByAddr::new(self, store)?,
        })
    }

    pub(crate) fn load_liquidity_stats_addr_may(
        &self,
        store: &dyn Storage,
        lp_addr: &Addr,
    ) -> Result<Option<LiquidityStatsByAddr>> {
        let mut cache = self.liquidity_cache.addr.borrow_mut();
        if let Some(stats) = cache.get(lp_addr) {
            return Ok(stats.clone());
        }

        let stats = LIQUIDITY_STATS_BY_ADDR.may_load(store, lp_addr)?;
        cache.insert(lp_addr.clone(), stats.clone());
        Ok(stats)
    }

    pub(crate) fn save_liquidity_stats_addr(
        &self,
        store: &mut dyn Storage,
        lp_addr: &Addr,
        addr_stats: &LiquidityStatsByAddr,
    ) -> Result<()> {
        let mut cache = self.liquidity_cache.addr.borrow_mut();
        if addr_stats.is_empty() {
            LIQUIDITY_STATS_BY_ADDR.remove(store, lp_addr);
            cache.insert(lp_addr.clone(), None);
        } else {
            LIQUIDITY_STATS_BY_ADDR.save(store, lp_addr, addr_stats)?;
            cache.insert(lp_addr.clone(), Some(addr_stats.clone()));
        }
        Ok(())
    }

    /// Collect the addresses of the liquidity providers at the given start and
    /// with the given limit.
    pub(crate) fn liquidity_providers(
        &self,
        store: &dyn Storage,
        start: Option<&Addr>,
        limit: usize,
    ) -> Result<Vec<Addr>> {
        LIQUIDITY_STATS_BY_ADDR
            .keys(store, start.map(Bound::exclusive), None, Order::Ascending)
            .take(limit)
            .map(|item| item.map_err(|err| err.into()))
            .collect()
    }

    #[cfg(feature = "sanity")]
    /// Iterate over all data available
    pub(crate) fn iter_liquidity_stats_addrs<'a>(
        &self,
        store: &'a dyn Storage,
    ) -> Box<
        dyn Iterator<Item = cosmwasm_std::StdResult<(cosmwasm_std::Addr, LiquidityStatsByAddr)>>
            + 'a,
    > {
        LIQUIDITY_STATS_BY_ADDR.range(store, None, None, Order::Ascending)
    }

    /// Do we need to reset LP balances right now?
    ///
    /// This impacts two things:
    ///
    /// 1. The crank will preferentially process these items
    ///
    /// 2. Various parts of the protocol will be disabled while this is pending
    pub(crate) fn should_reset_lp_balances(&self, store: &dyn Storage) -> Result<bool> {
        RESET_LP_STATUS
            .may_load(store)
            .map(|x| x.is_some())
            .map_err(|e| e.into())
    }

    /// Close out some LP balances
    pub(crate) fn crank_reset_lp_balances(&self, ctx: &mut StateContext) -> Result<()> {
        let reset_lp_status = RESET_LP_STATUS
            .may_load(ctx.storage)?
            .context("crank_reset_lp_balances called, but RESET_LP_START_AFTER is empty")?;
        let addr = LIQUIDITY_STATS_BY_ADDR
            .keys(
                ctx.storage,
                reset_lp_status.as_bound(),
                None,
                Order::Ascending,
            )
            .next()
            .transpose()?;
        let addr = match addr {
            None => {
                // All done processing
                RESET_LP_STATUS.remove(ctx.storage);
                return Ok(());
            }
            Some(addr) => addr,
        };

        // Accrue any yields
        self.update_accrued_yield(ctx, &addr)?;

        // And now reset the LP balances for this provider to 0
        let mut stats = self.load_liquidity_stats_addr(ctx.storage, &addr)?;
        stats.lp = LpToken::zero();
        stats.xlp = LpToken::zero();
        stats.last_accrue_key = self.latest_yield_per_token(ctx.storage)?.0;

        stats.unstaking = None;
        stats.cooldown_ends = None;
        self.save_liquidity_stats_addr(ctx.storage, &addr, &stats)?;

        // And we're done! Move on.
        RESET_LP_STATUS.save(ctx.storage, &ResetLpStatus::LastSaw(addr))?;
        Ok(())
    }

    /// Block liquidity-touching operations while we're in the middle of
    /// resetting LP balances.
    pub(crate) fn ensure_not_resetting_lps(
        &self,
        ctx: &mut StateContext,
        msg: &ExecuteMsg,
    ) -> Result<()> {
        let touches_liquidity = match msg {
            ExecuteMsg::Owner(_) => false,
            ExecuteMsg::Receive { .. } => true,
            ExecuteMsg::OpenPosition { .. } => true,
            ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { .. } => true,
            ExecuteMsg::UpdatePositionAddCollateralImpactSize { .. } => true,
            ExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage { .. } => true,
            ExecuteMsg::UpdatePositionRemoveCollateralImpactSize { .. } => true,
            ExecuteMsg::UpdatePositionLeverage { .. } => true,
            ExecuteMsg::UpdatePositionMaxGains { .. } => true,
            ExecuteMsg::UpdatePositionTakeProfitPrice { .. } => true,
            ExecuteMsg::UpdatePositionStopLossPrice { .. } => true,
            #[allow(deprecated)]
            ExecuteMsg::SetTriggerOrder { .. } => true,
            ExecuteMsg::ClosePosition { .. } => true,
            ExecuteMsg::DepositLiquidity { .. } => true,
            ExecuteMsg::ReinvestYield { .. } => true,
            ExecuteMsg::WithdrawLiquidity { .. } => true,
            ExecuteMsg::ClaimYield {} => true,
            ExecuteMsg::StakeLp { .. } => true,
            ExecuteMsg::UnstakeXlp { .. } => true,
            ExecuteMsg::StopUnstakingXlp {} => true,
            ExecuteMsg::CollectUnstakedLp {} => true,
            ExecuteMsg::Crank { .. } => false,
            ExecuteMsg::NftProxy { .. } => true,
            ExecuteMsg::LiquidityTokenProxy { .. } => true,
            ExecuteMsg::TransferDaoFees { .. } => true,
            ExecuteMsg::CloseAllPositions {} => true,
            ExecuteMsg::PlaceLimitOrder { .. } => true,
            ExecuteMsg::CancelLimitOrder { .. } => true,
            ExecuteMsg::ProvideCrankFunds {} => false,
            ExecuteMsg::SetManualPrice { .. } => false,
            ExecuteMsg::PerformDeferredExec { .. } => true,
        };
        if touches_liquidity && self.should_reset_lp_balances(ctx.storage)? {
            Err(anyhow::anyhow!(
                "Protocol temporarily halted while we reset LP balances, please try again"
            ))
        } else {
            Ok(())
        }
    }
}
