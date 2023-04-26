use super::data::*;
use crate::{
    arbitrary::helpers::token_range_u128, extensions::*, market_wrapper::PerpsMarket,
    time::TimeJump, PerpsApp,
};
use msg::prelude::{DirectionToBase, LpToken, UnsignedDecimal};
use proptest::prelude::*;
use std::{cell::RefCell, rc::Rc};

impl LpDepositWithdraw {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "1000.0").prop_flat_map(move |collateral| {
            (0.1..1.0f32).prop_flat_map(move |deposit_perc| {
                (0.1..1.0f32).prop_flat_map(move |withdraw_perc| {
                    (0.25f64..10.0f64).prop_map(move |partial_liquifunding| {
                        let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
                        let collateral = market.token.convert_u128(collateral);
                        let deposit = market.token.mul_f32(collateral, deposit_perc);
                        let withdraw = market.token.mul_f32(deposit, withdraw_perc);

                        Self {
                            market: Rc::new(RefCell::new(market)),
                            collateral,
                            deposit,
                            withdraw,
                            time_jump: TimeJump::FractionalLiquifundings(partial_liquifunding),
                        }
                    })
                })
            })
        })
    }
}

impl XlpStakeUnstake {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        token_range_u128("0.1", "1000.0").prop_flat_map(move |deposit| {
            (0.1..1.0f32).prop_flat_map(move |stake_perc| {
                (0.1..1.0f32).prop_map(move |unstake_perc| {
                    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
                    let deposit = market.token.convert_u128(deposit);
                    let stake = market.token.mul_f32(deposit, stake_perc);
                    let unstake = market.token.mul_f32(stake, unstake_perc);

                    Self {
                        market: Rc::new(RefCell::new(market)),
                        deposit,
                        stake: LpToken::from_decimal256(stake.into_decimal256()),
                        unstake: LpToken::from_decimal256(unstake.into_decimal256()),
                    }
                })
            })
        })
    }
}

impl LpYield {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        any::<bool>().prop_flat_map(|close_position| {
            token_range_u128("6.0", "1000.0").prop_flat_map(move |pos_collateral| {
                token_range_u128("0.1", "1000.0").prop_flat_map(move |lp_deposit| {
                    (1.0f64..10.0f64).prop_flat_map(move |time_jump_liquifundings| {
                        prop_oneof![
                            proptest::strategy::Just(DirectionToBase::Long),
                            proptest::strategy::Just(DirectionToBase::Short)
                        ]
                        .prop_map(move |pos_direction| {
                            let market =
                                PerpsMarket::lp_prep(PerpsApp::new_cell().unwrap()).unwrap();
                            let pos_collateral = market.token.convert_u128(pos_collateral);
                            let lp_deposit = market.token.convert_u128(lp_deposit);

                            Self {
                                market: Rc::new(RefCell::new(market)),
                                pos_collateral,
                                pos_direction,
                                lp_deposit,
                                close_position,
                                time_jump_liquifundings,
                            }
                        })
                    })
                })
            })
        })
    }
}
