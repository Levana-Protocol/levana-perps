use perpswap::prelude::*;
use std::{cell::RefCell, fmt::Debug, rc::Rc};

use crate::{market_wrapper::PerpsMarket, time::TimeJump};

#[derive(Clone)]
pub struct LpDepositWithdraw {
    pub market: Rc<RefCell<PerpsMarket>>,
    pub collateral: NonZero<Collateral>,
    pub deposit: NonZero<Collateral>,
    pub withdraw: NonZero<Collateral>,
    pub time_jump: TimeJump,
}

impl Debug for LpDepositWithdraw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lp")
            .field("collateral", &self.collateral.to_string())
            .field("deposit", &self.deposit.to_string())
            .field("withdraw", &self.withdraw.to_string())
            .field("time_jump", &self.time_jump)
            .field("market-id", &self.market.borrow().id.as_str())
            .field("market-type", &self.market.borrow().id.get_market_type())
            .finish()
    }
}

#[derive(Clone)]
pub struct XlpStakeUnstake {
    pub market: Rc<RefCell<PerpsMarket>>,
    pub deposit: NonZero<Collateral>,
    pub stake: LpToken,
    pub unstake: LpToken,
}

impl Debug for XlpStakeUnstake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lp")
            .field("deposit", &self.deposit.to_string())
            .field("stake", &self.stake.to_string())
            .field("unstake", &self.unstake.to_string())
            .field("market-id", &self.market.borrow().id.as_str())
            .field("market-type", &self.market.borrow().id.get_market_type())
            .finish()
    }
}

#[derive(Clone)]
pub struct LpYield {
    pub market: Rc<RefCell<PerpsMarket>>,
    pub pos_collateral: NonZero<Collateral>,
    pub pos_direction: DirectionToBase,
    pub lp_deposit: NonZero<Collateral>,
    pub close_position: bool,
    pub time_jump_liquifundings: f64,
}

impl Debug for LpYield {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lp")
            .field("pos_collateral", &self.pos_collateral.to_string())
            .field("pos_direction", &self.pos_direction)
            .field("lp_deposit", &self.lp_deposit.to_string())
            .field("time_jump", &self.time_jump_liquifundings.to_string())
            .field("close_position", &self.close_position.to_string())
            .field("market-id", &self.market.borrow().id.as_str())
            .field("market-type", &self.market.borrow().id.get_market_type())
            .finish()
    }
}
