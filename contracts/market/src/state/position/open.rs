use crate::prelude::*;
use crate::state::history::trade::trade_volume_usd;
use msg::contracts::market::delta_neutrality_fee::DeltaNeutralityFeeReason;
use msg::contracts::market::entry::{PositionActionKind, SlippageAssert};
use msg::contracts::market::fees::events::FeeSource;
use msg::contracts::market::position::events::{
    calculate_position_collaterals, PositionAttributes, PositionOpenEvent, PositionTradingFee,
};
use msg::contracts::market::position::{
    CollateralAndUsd, LiquidationMargin, SignedCollateralAndUsd,
};

use super::LAST_POSITION_ID;

impl State<'_> {
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
    ) -> Result<PositionId> {
        self.ensure_not_stale(ctx.storage)?;

        let price_point = self.spot_price(ctx.storage, None)?;

        let market_type = self.market_id(ctx.storage)?.get_market_type();

        let leverage_to_base = leverage.into_signed(direction);

        let leverage_to_notional = leverage_to_base.into_notional(market_type);

        let notional_size_in_collateral =
            leverage_to_notional.checked_mul_collateral(collateral)?;
        let notional_size =
            notional_size_in_collateral.map(|x| price_point.collateral_to_notional(x));
        if let Some(slippage_assert) = slippage_assert {
            self.do_slippage_assert(ctx, slippage_assert, notional_size, market_type, None)?;
        }

        let counter_collateral = max_gains_in_quote.calculate_counter_collateral(
            market_type,
            collateral,
            notional_size_in_collateral,
            leverage_to_notional,
        )?;

        self.open_position(
            ctx,
            sender,
            collateral,
            counter_collateral,
            notional_size,
            stop_loss_override,
            take_profit_override,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_position(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        collateral: NonZero<Collateral>,
        counter_collateral: NonZero<Collateral>,
        notional_size: Signed<Notional>,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<PositionId> {
        let price_point = self.spot_price(ctx.storage, None)?;
        let market_type = self.market_id(ctx.storage)?.get_market_type();

        let notional_size_in_collateral =
            notional_size.map(|x| price_point.notional_to_collateral(x));

        // FEES
        // https://www.notion.so/levana-protocol/Levana-Well-funded-Perpetuals-Whitepaper-9805a6eba56d429b839f5551dbb65c40#75bb26a1439c4a81894c2aa399471263

        let config = &self.config;

        // create the position
        let last_pos_id = LAST_POSITION_ID.load(ctx.storage)?;
        let pos_id = PositionId(last_pos_id.u64() + 1);
        LAST_POSITION_ID.save(ctx.storage, &pos_id)?;

        let liquifunded_at = self.now();
        let next_liquifunding =
            liquifunded_at.plus_seconds(config.liquifunding_delay_seconds.into());
        let stale_at = next_liquifunding.plus_seconds(config.staleness_seconds.into());

        // Initial position, before taking out any trading fees
        let mut pos = Position {
            owner: owner.clone(),
            id: pos_id,
            active_collateral: collateral,
            deposit_collateral: SignedCollateralAndUsd::new(collateral.into_signed(), &price_point),
            trading_fee: CollateralAndUsd::default(),
            funding_fee: SignedCollateralAndUsd::default(),
            borrow_fee: CollateralAndUsd::default(),
            crank_fee: CollateralAndUsd::default(),
            delta_neutrality_fee: SignedCollateralAndUsd::default(),
            counter_collateral,
            notional_size,
            created_at: self.now(),
            liquifunded_at,
            next_liquifunding,
            stale_at,
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

        self.trade_history_add_volume(
            ctx,
            &pos.owner,
            trade_volume_usd(&pos, price_point, market_type)?,
        )?;

        // Validate leverage before removing trading fees from active collateral
        self.position_validate_leverage_data(
            self.market_type(ctx.storage)?,
            &pos,
            &price_point,
            None,
        )?;

        // Validate that we have sufficient deposit collateral
        self.validate_minimum_deposit_collateral(ctx.storage, collateral.raw())?;

        // Now charge the trading fee
        pos.trading_fee.checked_add_assign(
            config
                .calculate_trade_fee_open(notional_size_in_collateral, counter_collateral.raw())?,
            &price_point,
        )?;

        pos.active_collateral = pos
            .active_collateral
            .checked_sub(pos.trading_fee.collateral())?;

        // VALIDATION

        self.position_validate_liquidity(
            ctx.storage,
            pos.counter_collateral.raw(),
            pos.notional_size,
            Some(self.now()),
        )?;

        let notional_size = pos.notional_size;

        // mint the nft
        self.nft_mint(ctx, owner, pos_id.to_string())?;

        // if success - the notional value gets added to total notional open
        // and net open interest
        self.charge_delta_neutrality_fee(
            ctx,
            &mut pos,
            notional_size,
            price_point,
            DeltaNeutralityFeeReason::PositionOpen,
        )?;
        self.adjust_net_open_interest(ctx, notional_size, pos.direction(), true)?;

        // lock the LP collateral
        self.liquidity_lock(ctx, pos.counter_collateral)?;

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

        // Save the position, setting liquidation margin and prices
        self.position_save(ctx, &mut pos, &price_point, false, true)?;

        // Check for sufficient margin
        perp_ensure!(
            pos.active_collateral.raw() >= pos.liquidation_margin.total(),
            ErrorId::InsufficientMargin,
            ErrorDomain::Market,
            "insufficient margin, active collateral: {}, liquidation_margin: {:?}",
            pos.active_collateral,
            pos.liquidation_margin,
        );

        // Now that we know the liquidation and max gains, confirm that the user
        // specified trigger orders are valid
        self.position_validate_trigger_orders(&pos, market_type, price_point)?;

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
            price_point,
        )?;

        ctx.response_mut().add_event(PositionOpenEvent {
            position_attributes: PositionAttributes {
                pos_id: pos.id,
                owner: pos.owner,
                collaterals,
                trading_fee,
                market_type,
                notional_size,
                notional_size_in_collateral: notional_size
                    .map(|notional_size| price_point.notional_to_collateral(notional_size)),
                notional_size_usd: notional_size
                    .map(|notional_size| price_point.notional_to_usd(notional_size)),
                direction,
                leverage,
                counter_leverage,
                stop_loss_override,
                take_profit_override,
            },
            created_at: pos.created_at,
        });

        Ok(pos_id)
    }
}
