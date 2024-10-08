use anyhow::{anyhow, Result};
use perpswap::contracts::market::position::{
    ClosedPosition, LiquidationReason, PositionCloseReason,
};

pub(crate) fn position_liquidated_reason(pos: &ClosedPosition) -> Result<LiquidationReason> {
    assert!(pos.liquidation_margin.is_some());
    match pos.reason {
        PositionCloseReason::Liquidated(reason) => Ok(reason),
        _ => Err(anyhow!("position should have been liquidated/take-profit")),
    }
}

pub fn assert_position_liquidated_reason(
    pos: &ClosedPosition,
    expected_reason: LiquidationReason,
) -> Result<()> {
    let reason = position_liquidated_reason(pos)?;
    if reason == expected_reason {
        Ok(())
    } else {
        anyhow::bail!(
            "position should have been liquidated (with reason {expected_reason:#?}), instead reason is {reason:#?}"
        );
    }
}

pub fn assert_position_liquidated(pos: &ClosedPosition) -> Result<()> {
    assert_position_liquidated_reason(pos, LiquidationReason::Liquidated)
}

pub fn assert_position_max_gains(pos: &ClosedPosition) -> Result<()> {
    assert_position_liquidated_reason(pos, LiquidationReason::MaxGains)
}

pub fn assert_position_stop_loss(pos: &ClosedPosition) -> Result<()> {
    assert_position_liquidated_reason(pos, LiquidationReason::StopLoss)
}

pub fn assert_position_take_profit(pos: &ClosedPosition) -> Result<()> {
    assert_position_liquidated_reason(pos, LiquidationReason::TakeProfit)
}
