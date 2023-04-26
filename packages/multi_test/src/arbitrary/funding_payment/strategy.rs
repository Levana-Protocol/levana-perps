use super::data::FundingPayment;
use crate::{
    arbitrary::helpers::token_range_u128, extensions::TokenExt, market_wrapper::PerpsMarket,
    time::TimeJump, PerpsApp,
};
use proptest::prelude::*;
use std::{cell::RefCell, rc::Rc};

impl FundingPayment {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        token_range_u128("10.0", "100.0").prop_flat_map(move |long_collateral| {
            token_range_u128("10.0", "100.0").prop_flat_map(move |short_collateral| {
                (0.25f64..10.0f64).prop_flat_map(move |partial_liquifunding| {
                    (0i64..1000i64).prop_map(move |blocks_between_closes| {
                        let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
                        let long_collateral = market.token.convert_u128(long_collateral);
                        let short_collateral = market.token.convert_u128(short_collateral);

                        Self {
                            market: Rc::new(RefCell::new(market)),
                            long_collateral,
                            short_collateral,
                            time_jump: TimeJump::FractionalLiquifundings(partial_liquifunding),
                            time_jump_between_closes: TimeJump::Blocks(blocks_between_closes),
                        }
                    })
                })
            })
        })
    }
}
