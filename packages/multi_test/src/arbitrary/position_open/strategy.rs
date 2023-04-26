use super::data::PositionOpen;
use crate::arbitrary::helpers::token_range_u128;
use crate::extensions::TokenExt;
use crate::{
    arbitrary::helpers::max_gains_strategy, config::DEFAULT_MARKET, market_wrapper::PerpsMarket,
    PerpsApp,
};
use msg::contracts::market::entry::SlippageAssert;
use msg::prelude::*;
use proptest::prelude::*;
use std::ops::RangeInclusive;
use std::rc::Rc;

impl PositionOpen {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        let (config, price_point) = {
            let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
            let config = market.query_config().unwrap();
            let price_point = market.query_current_price().unwrap();
            (config, price_point)
        };

        let direction = || {
            prop_oneof![
                proptest::strategy::Just(DirectionToBase::Long),
                proptest::strategy::Just(DirectionToBase::Short)
            ]
        };

        let min_collateral = price_point
            .usd_to_collateral(config.minimum_deposit_usd)
            .to_string();

        let collateral = (min_collateral, "100.0".to_string());

        // TODO: a different strategy that Just's all the other stuff
        // and varies only this
        let slippage_assert = || proptest::strategy::Just(None);

        let leverage = |direction: DirectionToBase| {
            let range: RangeInclusive<f32> = match DEFAULT_MARKET.collateral_type {
                MarketType::CollateralIsQuote => 0.25f32..=30.0f32,
                MarketType::CollateralIsBase => match direction {
                    DirectionToBase::Long => 1.25f32..=30.0f32,
                    DirectionToBase::Short => 0.25f32..=30.0f32,
                },
            };

            range.prop_map(|amount| amount.to_string().parse().unwrap())
        };

        let max_gains = move |direction: DirectionToBase, leverage_base: LeverageToBase| {
            max_gains_strategy(
                direction,
                leverage_base,
                DEFAULT_MARKET.collateral_type,
                &config,
            )
        };

        Self::new_strategy_inner(direction, collateral, slippage_assert, leverage, max_gains)
    }

    fn new_strategy_inner<A, SA, B, SB, C, SC, D, SD>(
        direction: A,
        collateral: (String, String),
        slippage_assert: B,
        leverage: C,
        max_gains: D,
    ) -> impl Strategy<Value = Self>
    where
        A: Clone + Fn() -> SA,
        SA: Strategy<Value = DirectionToBase>,
        B: Clone + Fn() -> SB,
        SB: Strategy<Value = Option<SlippageAssert>>,
        C: Clone + Fn(DirectionToBase) -> SC,
        SC: Strategy<Value = LeverageToBase>,
        D: Clone + Fn(DirectionToBase, LeverageToBase) -> SD,
        SD: Strategy<Value = MaxGainsInQuote>,
    {
        direction().prop_flat_map(move |direction| {
            token_range_u128(&collateral.0, &collateral.1).prop_flat_map({
                let leverage = leverage.clone();
                let max_gains = max_gains.clone();
                let slippage_assert = slippage_assert.clone();
                move |collateral| {
                    slippage_assert().prop_flat_map({
                        let leverage = leverage.clone();
                        let max_gains = max_gains.clone();
                        move |slippage_assert| {
                            leverage(direction).prop_flat_map({
                                let max_gains = max_gains.clone();
                                move |leverage| {
                                    max_gains(direction, leverage).prop_map({
                                        let slippage_assert = slippage_assert.clone();
                                        move |max_gains| {
                                            let market =
                                                PerpsMarket::new(PerpsApp::new_cell().unwrap())
                                                    .unwrap();

                                            let collateral = market.token.convert_u128(collateral);
                                            let market = Rc::new(market);

                                            Self {
                                                market,
                                                collateral,
                                                slippage_assert: slippage_assert.clone(),
                                                leverage,
                                                direction,
                                                max_gains,
                                                stop_loss_override: None,
                                                take_profit_override: None,
                                            }
                                        }
                                    })
                                }
                            })
                        }
                    })
                }
            })
        })
    }
}
