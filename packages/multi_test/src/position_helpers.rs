use anyhow::{anyhow, Result};
use msg::contracts::market::position::{ClosedPosition, LiquidationReason, PositionCloseReason};

pub(crate) fn position_liquidated_reason(pos: &ClosedPosition) -> Result<LiquidationReason> {
    match pos.reason {
        PositionCloseReason::Liquidated(reason) => Ok(reason),
        _ => Err(anyhow!("position should have been liquidated/take-profit")),
    }
}

pub fn assert_position_liquidated(pos: &ClosedPosition) -> Result<()> {
    match position_liquidated_reason(pos)? {
        LiquidationReason::Liquidated => Ok(()),
        _ => Err(anyhow!(
            "position should have been liquidated (with reason liquidated)"
        )),
    }
}

pub fn assert_position_max_gains(pos: &ClosedPosition) -> Result<()> {
    match position_liquidated_reason(pos)? {
        LiquidationReason::MaxGains => Ok(()),
        _ => Err(anyhow!(
            "position should have been liquidated (with reason max gains)"
        )),
    }
}

pub fn assert_position_stop_loss(pos: &ClosedPosition) -> Result<()> {
    match position_liquidated_reason(pos)? {
        LiquidationReason::StopLoss => Ok(()),
        _ => Err(anyhow!(
            "position should have been liquidated (with reason stop loss)"
        )),
    }
}

pub fn assert_position_take_profit(pos: &ClosedPosition) -> Result<()> {
    match position_liquidated_reason(pos)? {
        LiquidationReason::TakeProfit => Ok(()),
        _ => Err(anyhow!(
            "position should have been liquidated (with reason take profit)"
        )),
    }
}
