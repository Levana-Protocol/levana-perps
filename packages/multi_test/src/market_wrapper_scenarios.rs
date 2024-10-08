use crate::{config::DEFAULT_MARKET, market_wrapper::PerpsMarket, PerpsApp};
use perpswap::contracts::market::config::ConfigUpdate;
use perpswap::contracts::market::position::PositionQueryResponse;
use perpswap::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

impl PerpsMarket {
    pub fn new_open_position_long_1(
        app: Rc<RefCell<PerpsApp>>,
    ) -> Result<(Self, Addr, PositionQueryResponse)> {
        let market = Self::new_with_type(
            app,
            DEFAULT_MARKET.collateral_type,
            true,
            DEFAULT_MARKET.spot_price,
        )?;

        let trader = market.clone_trader(0).unwrap();
        let lp = market.clone_lp(0).unwrap();

        // make sure there's enough LP
        market
            .exec_mint_and_deposit_liquidity(&lp, 1_000_000_000u128.into())
            .unwrap();

        let (pos_id, _) = market
            .exec_open_position(
                &trader,
                "100",
                "10",
                DirectionToBase::Long,
                "1.0",
                None,
                None,
                None,
            )
            .unwrap();

        market.exec_refresh_price()?;
        market.exec_crank_till_finished(&trader)?;

        let pos = market.query_position(pos_id)?;

        Ok((market, trader, pos))
    }

    pub fn lp_prep(app: Rc<RefCell<PerpsApp>>) -> Result<Self> {
        let market = Self::new_with_type(
            app,
            DEFAULT_MARKET.collateral_type,
            false,
            DEFAULT_MARKET.spot_price,
        )?;

        // Ensure a fixed borrow fee rate to simplify calculations here
        market
            .exec_set_config(ConfigUpdate {
                borrow_fee_rate_min_annualized: Some("0.01".parse().unwrap()),
                borrow_fee_rate_max_annualized: Some("0.01".parse().unwrap()),
                delta_neutrality_fee_tax: Some(Decimal256::zero()),
                ..Default::default()
            })
            .unwrap();

        Ok(market)
    }
}
