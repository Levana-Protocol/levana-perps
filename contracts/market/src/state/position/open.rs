use crate::prelude::*;
use crate::state::delta_neutrality_fee::ChargeDeltaNeutralityFeeResult;
use crate::state::history::trade::trade_volume_usd;
use msg::contracts::market::delta_neutrality_fee::DeltaNeutralityFeeReason;
use msg::contracts::market::entry::{PositionActionKind, SlippageAssert};
use msg::contracts::market::fees::events::FeeSource;
use msg::contracts::market::position::events::{
    calculate_position_collaterals, PositionAttributes, PositionOpenEvent, PositionSaveReason,
    PositionTradingFee,
};
use msg::contracts::market::position::{
    CollateralAndUsd, LiquidationMargin, SignedCollateralAndUsd,
};

use super::{AdjustOpenInterestResult, LAST_POSITION_ID};

/// Information on a validated position we would like to open.
///
/// When opening a limit order, we do not want to write anything to storage until we've validated that the parameters are accurate. Otherwise, we'll end up with "zombie" information: traces of a position which we tried to open but didn't succeed with. To make this possible, we split position opening into two steps:
///
/// 1. Read only validation, which returns a value of this data type
///
/// 2. Writing that data to storage
pub(crate) struct ValidatedPosition {
    pos: Position,
    trade_volume_usd: Usd,
    price_point: PricePoint,
    delta_neutrality_fee: ChargeDeltaNeutralityFeeResult,
    open_interest: AdjustOpenInterestResult,
}

/// Parameters for opening a new position
pub(crate) struct OpenPositionParams {
    pub(crate) owner: Addr,
    pub(crate) collateral: NonZero<Collateral>,
    /// Crank fee already charged by the deferred execution system.
    pub(crate) crank_fee: CollateralAndUsd,
    pub(crate) leverage: LeverageToBase,
    pub(crate) direction: DirectionToBase,
    pub(crate) max_gains_in_quote: MaxGainsInQuote,
    pub(crate) slippage_assert: Option<SlippageAssert>,
    pub(crate) stop_loss_override: Option<PriceBaseInQuote>,
    pub(crate) take_profit_override: Option<PriceBaseInQuote>,
}

impl State<'_> {
    /// Try to validate a new position.
    pub(crate) fn validate_new_position(
        &self,
        store: &dyn Storage,
        OpenPositionParams {
            owner,
            collateral,
            crank_fee,
            leverage,
            direction,
            max_gains_in_quote,
            slippage_assert,
            stop_loss_override,
            take_profit_override,
        }: OpenPositionParams,
        price_point: &PricePoint,
    ) -> Result<ValidatedPosition> {
        let market_type = self.market_id(store)?.get_market_type();

        let leverage_to_base = leverage.into_signed(direction);

        let leverage_to_notional = leverage_to_base.into_notional(market_type);

        let notional_size_in_collateral =
            leverage_to_notional.checked_mul_collateral(collateral)?;
        let notional_size =
            notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x));
        if let Some(slippage_assert) = slippage_assert {
            self.do_slippage_assert(
                store,
                slippage_assert,
                notional_size,
                market_type,
                None,
                price_point,
            )?;
        }

        let counter_collateral = max_gains_in_quote.calculate_counter_collateral(
            market_type,
            collateral,
            notional_size_in_collateral,
            leverage_to_notional,
        )?;

        // FEES
        // https://www.notion.so/levana-protocol/Levana-Well-funded-Perpetuals-Whitepaper-9805a6eba56d429b839f5551dbb65c40#75bb26a1439c4a81894c2aa399471263

        let config = &self.config;

        // create the position
        let last_pos_id = LAST_POSITION_ID.load(store)?;
        let pos_id = PositionId::new(last_pos_id.u64() + 1);

        let liquifunded_at = price_point.timestamp;
        let next_liquifunding =
            liquifunded_at.plus_seconds(config.liquifunding_delay_seconds.into());

        // Initial position, before taking out any trading fees
        let mut pos = Position {
            owner,
            id: pos_id,
            active_collateral: collateral,
            deposit_collateral: SignedCollateralAndUsd::new(collateral.into_signed(), price_point),
            trading_fee: CollateralAndUsd::default(),
            funding_fee: SignedCollateralAndUsd::default(),
            borrow_fee: CollateralAndUsd::default(),
            crank_fee,
            pending_crank_fee: Usd::zero(),
            delta_neutrality_fee: SignedCollateralAndUsd::default(),
            counter_collateral,
            notional_size,
            created_at: self.now(),
            liquifunded_at,
            next_liquifunding,
            stop_loss_override,
            take_profit_override,
            liquidation_margin: LiquidationMargin::default(),
            liquidation_price: None,
            take_profit_price: None,
            stop_loss_override_notional: stop_loss_override
                .map(|x| x.into_notional_price(market_type)),
            take_profit_override_notional: take_profit_override
                .map(|x| x.into_notional_price(market_type)),
        };

        self.set_next_liquifunding_and_stale_at(&mut pos, liquifunded_at);

        let trade_volume_usd = trade_volume_usd(&pos, price_point, market_type)?;

        // Validate leverage before removing trading fees from active collateral
        self.position_validate_leverage_data(market_type, &pos, price_point, None)?;

        // Validate that we have sufficient deposit collateral
        self.validate_minimum_deposit_collateral(collateral.raw(), price_point)?;

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

        let delta_neutrality_fee = self.charge_delta_neutrality_fee(
            store,
            &mut pos,
            notional_size,
            price_point,
            DeltaNeutralityFeeReason::PositionOpen,
        )?;

        self.check_unlocked_liquidity(
            store,
            pos.counter_collateral,
            Some(pos.notional_size),
            price_point,
        )?;

        pos.liquidation_margin = pos.liquidation_margin(price_point, &self.config)?;

        // Check for sufficient margin
        perp_ensure!(
            pos.active_collateral.raw() >= pos.liquidation_margin.total(),
            ErrorId::InsufficientMargin,
            ErrorDomain::Market,
            "insufficient margin, active collateral: {}, liquidation_margin: {:?}",
            pos.active_collateral,
            pos.liquidation_margin,
        );

        let open_interest =
            self.check_adjust_net_open_interest(store, pos.notional_size, pos.direction(), true)?;

        // Now that we know the liquidation and max gains, confirm that the user
        // specified trigger orders are valid
        self.position_validate_trigger_orders(&pos, market_type, price_point)?;

        Ok(ValidatedPosition {
            pos,
            trade_volume_usd,
            price_point: *price_point,
            delta_neutrality_fee,
            open_interest,
        })
    }

    /// Write a validated position to storage.
    pub(crate) fn open_validated_position(
        &self,
        ctx: &mut StateContext,
        ValidatedPosition {
            mut pos,
            trade_volume_usd,
            price_point,
            delta_neutrality_fee,
            open_interest,
        }: ValidatedPosition,
        is_market: bool,
    ) -> Result<PositionId> {
        self.trade_history_add_volume(ctx, &pos.owner, trade_volume_usd)?;
        open_interest.store(ctx)?;

        // derive the new funding rate
        let funding_timestamp = self.funding_valid_until(ctx.storage)?;
        self.accumulate_funding_rate(ctx, funding_timestamp)?;

        // collect trading fees
        self.collect_trading_fee(
            ctx,
            pos.id,
            pos.trading_fee.collateral(),
            price_point,
            FeeSource::Trading,
        )?;

        delta_neutrality_fee.store(self, ctx)?;

        // Note that in the validity check we've already confirmed there is sufficient liquidity
        self.liquidity_lock(ctx, pos.counter_collateral, &price_point)?;

        // Save the position, setting liquidation margin and prices
        self.position_save(
            ctx,
            &mut pos,
            &price_point,
            false,
            true,
            if is_market {
                PositionSaveReason::OpenMarket
            } else {
                PositionSaveReason::ExecuteLimitOrder
            },
        )?;

        // mint the nft
        self.nft_mint(ctx, pos.owner.clone(), pos.id.to_string())?;
        LAST_POSITION_ID.save(ctx.storage, &pos.id)?;

        let market_id = self.market_id(ctx.storage)?;
        let market_type = market_id.get_market_type();
        let collaterals = calculate_position_collaterals(&pos)?;
        let trading_fee = pos.trading_fee.collateral();
        let trading_fee = PositionTradingFee {
            trading_fee,
            trading_fee_usd: price_point.collateral_to_usd(trading_fee),
        };

        let (direction, leverage) = pos
            .active_leverage_to_notional(&price_point)
            .into_base(market_type)
            .split();
        let (_, counter_leverage) = pos
            .counter_leverage_to_notional(&price_point)
            .into_base(market_type)
            .split();

        self.position_history_add_action(
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
                take_profit_override: pos.take_profit_override,
            },
            created_at: pos.created_at,
        });

        Ok(pos.id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn handle_position_open(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        collateral: NonZero<Collateral>,
        leverage: LeverageToBase,
        direction: DirectionToBase,
        max_gains_in_quote: MaxGainsInQuote,
        slippage_assert: Option<SlippageAssert>,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
        crank_fee: Collateral,
        crank_fee_usd: Usd,
        price_point: &PricePoint,
    ) -> Result<PositionId> {
        let validated_position = self.validate_new_position(
            ctx.storage,
            OpenPositionParams {
                owner: sender,
                collateral,
                leverage,
                direction,
                max_gains_in_quote,
                slippage_assert,
                stop_loss_override,
                take_profit_override,
                crank_fee: CollateralAndUsd::from_pair(crank_fee, crank_fee_usd),
            },
            price_point,
        )?;

        self.open_validated_position(ctx, validated_position, true)
    }
}
