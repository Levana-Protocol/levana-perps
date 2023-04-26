use msg::prelude::*;
use std::{cell::RefCell, fmt::Debug, rc::Rc};

use crate::{market_wrapper::PerpsMarket, time::TimeJump};

#[derive(Clone)]
pub struct FundingPayment {
    pub market: Rc<RefCell<PerpsMarket>>,
    pub long_collateral: NonZero<Collateral>,
    pub short_collateral: NonZero<Collateral>,
    pub time_jump: TimeJump,
    pub time_jump_between_closes: TimeJump,
}

impl Debug for FundingPayment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FundingPayment")
            .field("long_collateral", &self.long_collateral.to_string())
            .field("short_collateral", &self.short_collateral.to_string())
            .field("time_jump", &self.time_jump)
            .field("time_jump_between_closes", &self.time_jump_between_closes)
            .field("market-id", &self.market.borrow().id.as_str())
            .field("market-type", &self.market.borrow().id.get_market_type())
            .finish()
    }
}
