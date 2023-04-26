use crate::market_wrapper::PerpsMarket;
use msg::contracts::market::entry::SlippageAssert;
use msg::prelude::*;
use std::{fmt::Debug, rc::Rc};

#[derive(Clone)]
pub struct PositionOpen {
    pub market: Rc<PerpsMarket>,
    pub collateral: NonZero<Collateral>,
    pub slippage_assert: Option<SlippageAssert>,
    pub leverage: LeverageToBase,
    pub direction: DirectionToBase,
    pub max_gains: MaxGainsInQuote,
    pub stop_loss_override: Option<PriceBaseInQuote>,
    pub take_profit_override: Option<PriceBaseInQuote>,
}

impl Debug for PositionOpen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionOpen")
            .field("collateral", &self.collateral.to_string())
            .field("slippage_assert", &self.slippage_assert)
            .field("leverage", &self.leverage.to_string())
            .field("direction", &self.direction)
            .field("max_gains", &self.max_gains.to_string())
            .field("stop_loss_override", &self.stop_loss_override)
            .field("take_profit_override", &self.take_profit_override)
            .field("market-id", &self.market.id.as_str())
            .field("market-type", &self.market.id.get_market_type())
            .finish()
    }
}
