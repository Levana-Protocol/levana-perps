use crate::market_wrapper::{DeferResponse, PerpsMarket};
use msg::contracts::market::position::PositionId;
use msg::prelude::*;

use super::data::{
    PositionUpdateAddCollateralImpactLeverage, PositionUpdateAddCollateralImpactSize,
    PositionUpdateLeverage, PositionUpdateMaxGains, PositionUpdateRemoveCollateralImpactLeverage,
    PositionUpdateRemoveCollateralImpactSize,
};

pub enum UpdateExpect {
    Success,
    FailSlippage,
}

impl UpdateExpect {
    pub fn validate(
        &self,
        market: &PerpsMarket,
        res: Result<DeferResponse>,
        pos_id: PositionId,
    ) -> Result<()> {
        let _pos = market.query_position(pos_id)?;

        match self {
            Self::Success => {
                res?;
            }
            Self::FailSlippage => {
                let e = match res {
                    Ok(_) => bail!("Update should fail here due to slippage, but it succeeded"),
                    Err(e) => e,
                };
                let perp: PerpError = e.downcast().context("Not a PerpError")?;
                match perp.id {
                    ErrorId::SlippageAssert => (),
                    e => anyhow::bail!("Unexpected error ID: {e:?}"),
                }
            }
        }

        Ok(())
    }
}

impl PositionUpdateAddCollateralImpactLeverage {
    pub fn run(&self, expect: UpdateExpect) -> Result<()> {
        let market = &self.market;

        let res = market.exec_update_position_collateral_impact_leverage(
            &self.trader,
            self.pos_id,
            self.amount.into_signed(),
        );

        expect.validate(market, res, self.pos_id)?;

        Ok(())
    }
}

impl PositionUpdateAddCollateralImpactSize {
    pub fn run(&self, expect: UpdateExpect) -> Result<()> {
        let market = &self.market;

        let res = market.exec_update_position_collateral_impact_size(
            &self.trader,
            self.pos_id,
            self.amount.into_signed(),
            self.slippage_assert.clone(),
        );

        expect.validate(market, res, self.pos_id)?;

        Ok(())
    }
}

impl PositionUpdateRemoveCollateralImpactLeverage {
    pub fn run(&self, expect: UpdateExpect) -> Result<()> {
        let market = &self.market;

        let res = market.exec_update_position_collateral_impact_leverage(
            &self.trader,
            self.pos_id,
            -self.amount.into_signed(),
        );

        expect.validate(market, res, self.pos_id)?;

        Ok(())
    }
}

impl PositionUpdateRemoveCollateralImpactSize {
    pub fn run(&self, expect: UpdateExpect) -> Result<()> {
        let market = &self.market;

        let res = market.exec_update_position_collateral_impact_size(
            &self.trader,
            self.pos_id,
            -self.amount.into_signed(),
            self.slippage_assert.clone(),
        );

        expect.validate(market, res, self.pos_id)?;

        Ok(())
    }
}

impl PositionUpdateLeverage {
    pub fn run(&self, expect: UpdateExpect) -> Result<()> {
        let market = &self.market;

        let res = market.exec_update_position_leverage(
            &self.trader,
            self.pos_id,
            self.leverage,
            self.slippage_assert.clone(),
        );

        expect.validate(market, res, self.pos_id)?;

        Ok(())
    }
}

impl PositionUpdateMaxGains {
    pub fn run(&self, expect: UpdateExpect) -> Result<()> {
        let market = &self.market;

        let res = market.exec_update_position_max_gains(&self.trader, self.pos_id, self.max_gains);

        expect.validate(market, res, self.pos_id)?;

        Ok(())
    }
}
