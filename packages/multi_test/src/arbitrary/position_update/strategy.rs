use super::data::{
    PositionUpdateAddCollateralImpactLeverage, PositionUpdateAddCollateralImpactSize,
    PositionUpdateLeverage, PositionUpdateMaxGains, PositionUpdateRemoveCollateralImpactLeverage,
    PositionUpdateRemoveCollateralImpactSize,
};
use crate::arbitrary::helpers::token_range_u128;
use crate::extensions::TokenExt;
use crate::{
    arbitrary::helpers::max_gains_strategy, config::DEFAULT_MARKET, market_wrapper::PerpsMarket,
    PerpsApp,
};
use perpswap::contracts::market::entry::SlippageAssert;
use perpswap::prelude::*;
use proptest::prelude::*;
use std::rc::Rc;

impl PositionUpdateRemoveCollateralImpactLeverage {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "60.0").prop_map(move |amount| {
            let (market, trader, pos) =
                PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
            let amount = market.token.convert_u128(amount);
            let market = Rc::new(market);
            Self {
                market,
                amount,
                pos_id: pos.id,
                trader,
            }
        })
    }
}

impl PositionUpdateAddCollateralImpactLeverage {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "10000.0").prop_map(move |amount| {
            let (market, trader, pos) =
                PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
            let amount = market.token.convert_u128(amount);
            let market = Rc::new(market);

            Self {
                market,
                amount,
                pos_id: pos.id,
                trader,
            }
        })
    }
}

impl PositionUpdateRemoveCollateralImpactSize {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "60.0").prop_map(move |amount| {
            let (market, trader, pos) =
                PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
            let amount = market.token.convert_u128(amount);
            let market = Rc::new(market);

            Self {
                market,
                amount,
                slippage_assert: None,
                pos_id: pos.id,
                trader,
            }
        })
    }

    pub fn new_strategy_exceed_slippage() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "60.0").prop_map(move |amount| {
            let (market, trader, pos) =
                PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
            let amount = market.token.convert_u128(amount);
            let market = Rc::new(market);

            let market_price = market.query_current_price().unwrap();

            let slippage_assert = Some(SlippageAssert {
                price: PriceBaseInQuote::try_from_number(
                    (market_price.price_base.into_number() * Number::two()).unwrap(),
                )
                .unwrap(),
                tolerance: "0.0".parse().unwrap(),
            });

            Self {
                market,
                amount,
                slippage_assert,
                pos_id: pos.id,
                trader,
            }
        })
    }

    pub fn new_strategy_valid_slippage() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "60.0").prop_map(move |amount| {
            let (market, trader, pos) =
                PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
            let amount = market.token.convert_u128(amount);
            let market = Rc::new(market);

            let market_price = market.query_current_price().unwrap();
            let slippage_assert = Some(SlippageAssert {
                price: PriceBaseInQuote::try_from_number(
                    (market_price.price_base.into_number() * Number::two()).unwrap(),
                )
                .unwrap(),
                tolerance: "100000.0".parse().unwrap(),
            });

            Self {
                market,
                amount,
                slippage_assert,
                pos_id: pos.id,
                trader,
            }
        })
    }
}

impl PositionUpdateAddCollateralImpactSize {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "10000.0").prop_map(move |amount| {
            let (market, trader, pos) =
                PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
            let amount = market.token.convert_u128(amount);
            let market = Rc::new(market);

            Self {
                market,
                amount,
                slippage_assert: None,
                pos_id: pos.id,
                trader,
            }
        })
    }

    pub fn new_strategy_exceed_slippage() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "10000.0").prop_flat_map(move |amount| {
            (0.1f32..=0.89f32).prop_map(move |slippage_assert| {
                let (market, trader, pos) =
                    PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
                let amount = market.token.convert_u128(amount);
                let market = Rc::new(market);

                let market_price = market.query_current_price().unwrap();

                let slippage_assert = Some(SlippageAssert {
                    price: PriceBaseInQuote::try_from_number(
                        (market_price.price_base.into_number()
                            * Number::try_from(slippage_assert.to_string()).unwrap())
                        .unwrap(),
                    )
                    .unwrap(),
                    tolerance: "0.1".parse().unwrap(),
                });

                Self {
                    market,
                    amount,
                    slippage_assert,
                    pos_id: pos.id,
                    trader,
                }
            })
        })
    }

    pub fn new_strategy_valid_slippage() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "10000.0").prop_flat_map(move |amount| {
            (0.92f32..=1.1f32).prop_map(move |slippage_assert| {
                let (market, trader, pos) =
                    PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
                let amount = market.token.convert_u128(amount);
                let market = Rc::new(market);

                let market_price = market.query_current_price().unwrap();

                let slippage_assert = Some(SlippageAssert {
                    price: PriceBaseInQuote::try_from_number(
                        (market_price.price_base.into_number()
                            * Number::try_from(slippage_assert.to_string()).unwrap())
                        .unwrap(),
                    )
                    .unwrap(),
                    tolerance: "0.1".parse().unwrap(),
                });

                Self {
                    market,
                    amount,
                    slippage_assert,
                    pos_id: pos.id,
                    trader,
                }
            })
        })
    }
}

impl PositionUpdateLeverage {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        (1.25f32..30.0f32).prop_map(move |amount| {
            let (market, trader, pos) =
                PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
            let market = Rc::new(market);

            Self {
                market,
                leverage: amount.to_string().parse().unwrap(),
                slippage_assert: None,
                pos_id: pos.id,
                trader,
            }
        })
    }
}

impl PositionUpdateMaxGains {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        let (market, _, pos) =
            PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
        let config = market.query_config().unwrap();

        max_gains_strategy(
            DirectionToBase::Long,
            pos.leverage,
            DEFAULT_MARKET.collateral_type,
            &config,
        )
        .prop_map(move |max_gains| {
            let (market, trader, pos) =
                PerpsMarket::new_open_position_long_1(PerpsApp::new_cell().unwrap()).unwrap();
            let market = Rc::new(market);

            Self {
                market,
                max_gains,
                pos_id: pos.id,
                trader,
            }
        })
    }
}
