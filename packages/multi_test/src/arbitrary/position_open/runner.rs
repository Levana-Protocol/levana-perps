use crate::market_wrapper::{DeferResponse, PerpsMarket};
use anyhow::Result;
use msg::contracts::market::position::PositionId;

use super::data::PositionOpen;

pub enum OpenExpect {
    Success,
}

impl OpenExpect {
    pub fn validate(
        &self,
        market: &PerpsMarket,
        res: Result<(PositionId, DeferResponse)>,
    ) -> Result<()> {
        match self {
            Self::Success => {
                let (pos_id, _) = res?;
                let _pos = market.query_position(pos_id)?;
            }
        }

        Ok(())
    }
}

impl PositionOpen {
    pub fn run(&self, expect: OpenExpect) -> Result<()> {
        let market = &self.market;
        let trader = market.clone_trader(0)?;

        // open position
        let res = market.exec_open_position_raw(
            &trader,
            self.collateral.into_number(),
            self.slippage_assert.clone(),
            self.leverage,
            self.direction,
            self.max_gains,
            self.stop_loss_override,
            self.take_profit_override,
        );

        expect.validate(market, res)?;

        Ok(())
    }
}
