use std::{fmt::Debug, rc::Rc};

use cosmwasm_std::Addr;
use perpswap::contracts::market::{entry::SlippageAssert, position::PositionId};
use perpswap::prelude::*;

use crate::market_wrapper::PerpsMarket;

#[derive(Clone)]
pub struct PositionUpdateRemoveCollateralImpactLeverage {
    pub market: Rc<PerpsMarket>,
    pub amount: NonZero<Collateral>,
    pub pos_id: PositionId,
    pub trader: Addr,
}

#[derive(Clone)]
pub struct PositionUpdateRemoveCollateralImpactSize {
    pub market: Rc<PerpsMarket>,
    pub amount: NonZero<Collateral>,
    pub slippage_assert: Option<SlippageAssert>,
    pub pos_id: PositionId,
    pub trader: Addr,
}

#[derive(Clone)]
pub struct PositionUpdateAddCollateralImpactLeverage {
    pub market: Rc<PerpsMarket>,
    pub amount: NonZero<Collateral>,
    pub pos_id: PositionId,
    pub trader: Addr,
}

#[derive(Clone)]
pub struct PositionUpdateAddCollateralImpactSize {
    pub market: Rc<PerpsMarket>,
    pub amount: NonZero<Collateral>,
    pub slippage_assert: Option<SlippageAssert>,
    pub pos_id: PositionId,
    pub trader: Addr,
}

#[derive(Clone)]
pub struct PositionUpdateLeverage {
    pub market: Rc<PerpsMarket>,
    pub leverage: LeverageToBase,
    pub slippage_assert: Option<SlippageAssert>,
    pub pos_id: PositionId,
    pub trader: Addr,
}

#[derive(Clone)]
pub struct PositionUpdateMaxGains {
    pub market: Rc<PerpsMarket>,
    pub max_gains: MaxGainsInQuote,
    pub pos_id: PositionId,
    pub trader: Addr,
}

impl Debug for PositionUpdateRemoveCollateralImpactLeverage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionUpdateRemoveCollateralImpactLeverage")
            .field("amount", &self.amount.to_string())
            .field("pos_id", &self.pos_id)
            .field("market_id", &self.market.id)
            .field("market-type", &self.market.id.get_market_type())
            .field("trader", &self.trader.to_string())
            .finish()
    }
}

impl Debug for PositionUpdateRemoveCollateralImpactSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionUpdateRemoveCollateralImpactSize")
            .field("amount", &self.amount.to_string())
            .field(
                "slippage_assert",
                &debug_optional_slippage_assert(&self.slippage_assert),
            )
            .field("pos_id", &self.pos_id)
            .field("market_id", &self.market.id)
            .field("market-type", &self.market.id.get_market_type())
            .field("trader", &self.trader.to_string())
            .finish()
    }
}

impl Debug for PositionUpdateAddCollateralImpactLeverage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionUpdateAddCollateralImpactLeverage")
            .field("amount", &self.amount.to_string())
            .field("pos_id", &self.pos_id)
            .field("market_id", &self.market.id)
            .field("market-type", &self.market.id.get_market_type())
            .field("trader", &self.trader.to_string())
            .finish()
    }
}

impl Debug for PositionUpdateAddCollateralImpactSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionUpdateAddCollateralImpactSize")
            .field("amount", &self.amount.to_string())
            .field(
                "slippage_assert",
                &debug_optional_slippage_assert(&self.slippage_assert),
            )
            .field("pos_id", &self.pos_id)
            .field("market_id", &self.market.id)
            .field("market-type", &self.market.id.get_market_type())
            .field("trader", &self.trader.to_string())
            .finish()
    }
}

impl Debug for PositionUpdateLeverage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionUpdateLeverage")
            .field("leverage", &self.leverage.to_string())
            .field(
                "slippage_assert",
                &debug_optional_slippage_assert(&self.slippage_assert),
            )
            .field("pos_id", &self.pos_id)
            .field("market_id", &self.market.id)
            .field("market-type", &self.market.id.get_market_type())
            .field("trader", &self.trader.to_string())
            .finish()
    }
}

impl Debug for PositionUpdateMaxGains {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionUpdateMaxGains")
            .field("max_gains", &self.max_gains.to_string())
            .field("pos_id", &self.pos_id)
            .field("market_id", &self.market.id)
            .field("market-type", &self.market.id.get_market_type())
            .field("trader", &self.trader.to_string())
            .finish()
    }
}

fn debug_optional_slippage_assert(s: &Option<SlippageAssert>) -> String {
    match s {
        None => "".to_string(),
        Some(s) => format!("price: {}, tolerance: {}", s.price, s.tolerance),
    }
}
