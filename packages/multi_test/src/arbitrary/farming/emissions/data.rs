use msg::prelude::*;
use std::{cell::RefCell, fmt::Debug, rc::Rc};

use crate::market_wrapper::PerpsMarket;

#[derive(Clone)]
pub struct FarmingEmissions {
    pub market: Rc<RefCell<PerpsMarket>>,
    pub actions: Vec<Action>,
    pub emissions_duration_seconds: u32,
    pub emissions_amount: LvnToken,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub kind: ActionKind,
    pub at_seconds: u32,
}

#[derive(Clone, Debug)]
pub enum ActionKind {
    Deposit(Collateral),
    Withdraw(Collateral),
}

impl Debug for FarmingEmissions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FarmingEmissions")
            .field("market-id", &self.market.borrow().id.as_str())
            .field("market-type", &self.market.borrow().id.get_market_type())
            .field("actions", &self.actions)
            .field(
                "emissions_duration_seconds",
                &self.emissions_duration_seconds,
            )
            .field("emissions_amount", &self.emissions_amount)
            .finish()
    }
}
