use msg::{contracts::market::config::Config as MarketConfig, token::Token};

use crate::{page::home::app::stats::Stats, prelude::*};

pub struct EventView {
    pub stats: Rc<Stats>,
}

impl EventView {
    pub fn new(stats: Rc<Stats>) -> Rc<Self> {
        Rc::new(Self { stats })
    }
}
