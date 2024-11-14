use crate::prelude::*;
use crate::state::delta_neutrality_fee::ChargeDeltaNeutralityFeeResult;
use crate::state::history::trade::trade_volume_usd;
use crate::state::liquidity::LiquidityLock;
use crate::state::position::take_profit::TakeProfitToCounterCollateral;
use anyhow::ensure;
use perpswap::contracts::market::delta_neutrality_fee::DeltaNeutralityFeeReason;
use perpswap::contracts::market::entry::{PositionActionKind, SlippageAssert};
use perpswap::contracts::market::fees::events::FeeSource;
use perpswap::contracts::market::position::events::{
    calculate_position_collaterals, PositionAttributes, PositionOpenEvent, PositionSaveReason,
    PositionTradingFee,
};
use perpswap::contracts::market::position::{
    CollateralAndUsd, LiquidationMargin, SignedCollateralAndUsd,
};

use super::{AdjustOpenInterest, LAST_POSITION_ID};

/// Information on a validated position we would like to open.
///
/// When opening a limit order, we do not want to write anything to storage until we've validated that the parameters are accurate. Otherwise, we'll end up with "zombie" information: traces of a position which we tried to open but didn't succeed with. To make this possible, we split position opening into two steps:
///
/// 1. Read only validation, which returns a value of this data type
///
/// 2. Writing that data to storage
#[must_use]
pub(crate) struct OpenPositionExec {
    pos: Position,
    trade_volume_usd: Usd,
    price_point: PricePoint,
    delta_neutrality_fee: ChargeDeltaNeutralityFeeResult,
    open_interest: AdjustOpenInterest,
    liquidity: LiquidityLock,
}

impl OpenPositionExec {
    pub(crate) fn new(
        state: &State,
        store: &dyn Storage,
        OpenPositionParams {
            owner,
            collateral,
            crank_fee,
            leverage,
            direction,
            slippage_assert,
            stop_loss_override,
            take_profit_trader,
        }: OpenPositionParams,
        price_point: &PricePoint,
    ) -> Result<Self> {
        let market_type = state.market_id(store)?.get_market_type();

        let leverage_to_base = leverage.into_signed(direction);

        let leverage_to_notional = leverage_to_base.into_notional(market_type)?;

        let notional_size_in_collateral =
            leverage_to_notional.checked_mul_collateral(collateral)?;
        let notional_size =
            notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x));
        if let Some(slippage_assert) = slippage_assert {
            state.do_slippage_assert(
                store,
                slippage_assert,
                notional_size,
                market_type,
                None,
                price_point,
            )?;
        }

        let counter_collateral = TakeProfitToCounterCollateral {
            take_profit_trader,
            market_type,
            collateral,
            leverage_to_base: leverage,
            direction,
            config: &state.config,
            price_point,
        }
        .calc()?;

        // FEES
        // https://www.notion.so/levana-protocol/Levana-Well-funded-Perpetuals-Whitepaper-9805a6eba56d429b839f5551dbb65c40#75bb26a1439c4a81894c2aa399471263

        let config = &state.config;

        // create the position
        let last_pos_id = LAST_POSITION_ID.load(store)?;
        let pos_id = PositionId::new(last_pos_id.u64() + 1);

        let liquifunded_at = price_point.timestamp;

        // Initial position, before taking out any trading fees
        let mut pos = Position {
            owner,
            id: pos_id,
            active_collateral: collateral,
            deposit_collateral: SignedCollateralAndUsd::new(
                collateral
                    .checked_add(crank_fee.collateral())?
                    .into_signed(),
                price_point,
            ),
            trading_fee: CollateralAndUsd::default(),
            funding_fee: SignedCollateralAndUsd::default(),
            borrow_fee: CollateralAndUsd::default(),
            crank_fee,
            delta_neutrality_fee: SignedCollateralAndUsd::default(),
            counter_collateral,
            notional_size,
            created_at: state.now(),
            price_point_created_at: Some(price_point.timestamp),
            liquifunded_at,
            // just temporarily setting _something_ here, it will be overwritten right away in `set_next_liquifunding`
            next_liquifunding: liquifunded_at,
            stop_loss_override,
            liquidation_margin: LiquidationMargin::default(),
            liquidation_price: None,
            // We temporarily fill in a value of None. Later, during position save, we will calculate the correct value from the actual counter collateral amount.
            take_profit_total: None,
            take_profit_trader: Some(take_profit_trader),
            take_profit_trader_notional: take_profit_trader.into_notional(market_type),
            stop_loss_override_notional: stop_loss_override
                .map(|x| x.into_notional_price(market_type)),
        };

        state.set_next_liquifunding(&mut pos, liquifunded_at);

        let trade_volume_usd = trade_volume_usd(&pos, price_point, market_type)?;

        // Validate leverage before removing trading fees from active collateral
        state.position_validate_leverage_data(market_type, &pos, price_point, None)?;

        // Validate that we have sufficient deposit collateral
        state.validate_minimum_deposit_collateral(collateral.raw(), price_point)?;

        // Now charge the trading fee
        pos.trading_fee.checked_add_assign(
            config
                .calculate_trade_fee_open(notional_size_in_collateral, counter_collateral.raw())?,
            price_point,
        )?;

        pos.active_collateral = pos
            .active_collateral
            .checked_sub(pos.trading_fee.collateral())?;

        // VALIDATION

        let delta_neutrality_fee = state.charge_delta_neutrality_fee(
            store,
            &mut pos,
            notional_size,
            price_point,
            DeltaNeutralityFeeReason::PositionOpen,
        )?;

        let liquidity = LiquidityLock::new(
            state,
            store,
            pos.counter_collateral,
            *price_point,
            Some(pos.notional_size),
            None,
            None,
        )?;

        pos.liquidation_margin = pos.liquidation_margin(price_point, config)?;

        // Check for sufficient margin
        ensure!(
            pos.active_collateral.raw() >= pos.liquidation_margin.total()?,
            format!(
                "insufficient margin, active collateral: {}, liquidation_margin: {:?}",
                pos.active_collateral, pos.liquidation_margin
            )
        );

        let open_interest =
            AdjustOpenInterest::new(state, store, pos.notional_size, pos.direction(), true)?;

        Ok(Self {
            pos,
            trade_volume_usd,
            price_point: *price_point,
            delta_neutrality_fee,
            open_interest,
            liquidity,
        })
    }

    // This is a no-op, but it's more expressive to call discard() or apply()
    // rather than to just assign it to a throwaway variable.
    pub(crate) fn discard(self) {}

    pub(crate) fn apply(
        self,
        state: &State,
        ctx: &mut StateContext,
        save_reason: PositionSaveReason,
    ) -> Result<PositionId> {
        let Self {
            mut pos,
            trade_volume_usd,
            price_point,
            delta_neutrality_fee,
            open_interest,
            liquidity,
        } = self;
        state.trade_history_add_volume(ctx, &pos.owner, trade_volume_usd)?;

        open_interest.apply(ctx)?;

        // collect trading fees
        state.collect_trading_fee(
            ctx,
            pos.id,
            pos.trading_fee.collateral(),
            price_point,
            FeeSource::Trading,
            &pos.owner,
        )?;

        delta_neutrality_fee.apply(state, ctx)?;

        // Note that in the validity check we've already confirmed there is sufficient liquidity
        liquidity.apply(state, ctx)?;

        // Save the position, setting liquidation margin and prices
        state.position_save(ctx, &mut pos, &price_point, false, true, save_reason)?;

        // mint the nft
        state.nft_mint(ctx, pos.owner.clone(), pos.id.to_string())?;
        LAST_POSITION_ID.save(ctx.storage, &pos.id)?;

        let market_id = state.market_id(ctx.storage)?;
        let market_type = market_id.get_market_type();
        let collaterals = calculate_position_collaterals(&pos)?;
        let trading_fee = pos.trading_fee.collateral();
        let trading_fee = PositionTradingFee {
            trading_fee,
            trading_fee_usd: price_point.collateral_to_usd(trading_fee),
        };

        let (direction, leverage) = pos
            .active_leverage_to_notional(&price_point)
            .into_base(market_type)?
            .split();
        let (_, counter_leverage) = pos
            .counter_leverage_to_notional(&price_point)
            .into_base(market_type)?
            .split();

        state.position_history_add_open_update_action(
            ctx,
            &pos,
            PositionActionKind::Open,
            Some(pos.trading_fee.collateral()),
            Some(pos.delta_neutrality_fee.collateral()),
            pos.deposit_collateral.collateral(),
            price_point,
        )?;

        ctx.response_mut().add_event(PositionOpenEvent {
            position_attributes: PositionAttributes {
                pos_id: pos.id,
                owner: pos.owner,
                collaterals,
                trading_fee,
                market_type,
                notional_size: pos.notional_size,
                notional_size_in_collateral: pos
                    .notional_size
                    .map(|notional_size| price_point.notional_to_collateral(notional_size)),
                notional_size_usd: pos
                    .notional_size
                    .map(|notional_size| price_point.notional_to_usd(notional_size)),
                direction,
                leverage,
                counter_leverage,
                stop_loss_override: pos.stop_loss_override,
                take_profit_trader: pos.take_profit_trader,
            },
            created_at: pos.created_at,
            price_point_created_at: price_point.timestamp,
        });

        Ok(pos.id)
    }
}

/// Parameters for opening a new position
pub(crate) struct OpenPositionParams {
    pub(crate) owner: Addr,
    pub(crate) collateral: NonZero<Collateral>,
    /// Crank fee already charged by the deferred execution system.
    pub(crate) crank_fee: CollateralAndUsd,
    pub(crate) leverage: LeverageToBase,
    pub(crate) direction: DirectionToBase,
    pub(crate) slippage_assert: Option<SlippageAssert>,
    pub(crate) stop_loss_override: Option<PriceBaseInQuote>,
    pub(crate) take_profit_trader: TakeProfitTrader,
}
