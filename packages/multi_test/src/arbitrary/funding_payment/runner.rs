use std::cmp::Ordering;

use super::data::FundingPayment;
use crate::market_wrapper::PerpsMarket;
use anyhow::Result;
use cosmwasm_std::Addr;
use cw_multi_test::AppResponse;
use msg::contracts::market::position::PositionId;
use msg::prelude::*;

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum FundingPaymentExpect {
    Success,
}

impl FundingPaymentExpect {
    pub fn validate(
        &self,
        market: &PerpsMarket,
        res: Result<(PositionId, AppResponse)>,
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

impl FundingPayment {
    pub fn run(&self, expect: FundingPaymentExpect) -> Result<()> {
        let market = self.market.borrow_mut();
        //market.automatic_time_jump_enabled = false;
        let trader = market.clone_trader(0)?;

        // open long position
        let (long_id, _) = market.exec_open_position_raw(
            &trader,
            self.long_collateral.into_number(),
            None,
            "10".parse().unwrap(),
            DirectionToBase::Long,
            MaxGainsInQuote::Finite("1.0".parse().unwrap()),
            None,
            None,
        )?;

        // open short position
        let (short_id, _) = market.exec_open_position_raw(
            &trader,
            self.long_collateral.into_number(),
            None,
            "10".parse().unwrap(),
            DirectionToBase::Short,
            MaxGainsInQuote::Finite("1.0".parse().unwrap()),
            None,
            None,
        )?;

        let long_notional_size = market.query_position(long_id)?.notional_size.abs();
        let short_notional_size = market.query_position(short_id)?.notional_size.abs();

        market.set_time(self.time_jump)?;
        market.exec_refresh_price()?;

        market.exec_crank_till_finished(&Addr::unchecked("cranker"))?;

        market.exec_close_position(&trader, long_id, None).unwrap();
        market.exec_close_position(&trader, short_id, None).unwrap();

        let long = market.query_closed_position(&trader, long_id)?;

        // Add a time jump between closing a long and a short.
        market.set_time(self.time_jump_between_closes)?;
        market.exec_refresh_price()?;
        market.exec_crank_till_finished(&Addr::unchecked("cranker"))?;

        let short = market.query_closed_position(&trader, short_id)?;

        if expect == FundingPaymentExpect::Success {
            assert!(long
                .funding_fee_collateral
                .into_number()
                .abs()
                .approx_eq(short.funding_fee_collateral.abs().into_number()));

            match long_notional_size.cmp(&short_notional_size) {
                Ordering::Greater => {
                    assert!(long.funding_fee_collateral > short.funding_fee_collateral)
                }
                Ordering::Less => {
                    assert!(long.funding_fee_collateral < short.funding_fee_collateral)
                }
                Ordering::Equal => {}
            }
        }

        Ok(())
    }
}
