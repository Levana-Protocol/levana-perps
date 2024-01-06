use anyhow::{anyhow, Result};
use msg::contracts::market::position::{ClosedPosition, LiquidationReason, PositionCloseReason};

pub(crate) fn position_liquidated_reason(pos: &ClosedPosition) -> Result<LiquidationReason> {
    match pos.reason {
        PositionCloseReason::Liquidated(reason) => Ok(reason),
        _ => Err(anyhow!("position should have been liquidated/take-profit")),
    }
}

pub fn assert_position_liquidated(pos: &ClosedPosition) -> Result<()> {
    let reason = position_liquidated_reason(pos)?;
    match reason {
        LiquidationReason::Liquidated => Ok(()),
        _ => Err(anyhow!(
            "position should have been liquidated (with reason liquidated), instead reason is {reason:#?}"
        )),
    }
}

pub fn assert_position_max_gains(pos: &ClosedPosition) -> Result<()> {
    let reason = position_liquidated_reason(pos)?;
    match reason {
        LiquidationReason::MaxGains => Ok(()),
        _ => Err(anyhow!(
            "position should have been liquidated (with reason max gains), instead reason is {reason:#?}"
        )),
    }
}

pub fn assert_position_stop_loss(pos: &ClosedPosition) -> Result<()> {
    let reason = position_liquidated_reason(pos)?;
    match reason {
        LiquidationReason::StopLoss => Ok(()),
        _ => Err(anyhow!(
            "position should have been liquidated (with reason stop loss), instead reason is {reason:#?}"
        )),
    }
}

pub fn assert_position_take_profit(pos: &ClosedPosition) -> Result<()> {
    let reason = position_liquidated_reason(pos)?;
    match reason {
        LiquidationReason::TakeProfit => Ok(()),
        _ => Err(anyhow!(
            "position should have been liquidated (with reason take profit), instead reason is {reason:#?}"
        )),
    }
}
