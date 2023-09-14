use std::{
    collections::HashSet,
    ops::{Div, Mul, RangeInclusive, Sub},
};

use crate::{bridge::BridgeResponse, prelude::*};
use cosmwasm_std::Event as CosmosEvent;
use msg::{
    contracts::market::{
        config::Config as MarketConfig,
        entry::{ExecuteMsg, ExecuteOwnerMsg, QueryMsg, StatusResp},
        position::{PositionId, PositionQueryResponse, PositionsResp},
    },
    prelude::*,
    token::Token,
};
use rand::{distributions::uniform::SampleRange, prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    OpenPosition,
    UpdatePosition,
    ClosePosition,
    SetPrice,
    TimeJump,
}

impl Action {
    pub fn all() -> &'static [Action] {
        &[
            Action::OpenPosition,
            Action::UpdatePosition,
            Action::ClosePosition,
            Action::SetPrice,
            Action::TimeJump,
        ]
    }
}

pub enum ActionLog {
    Execute {
        msg_id: u64,
        msg_elapsed: f64,
        exec: ExecuteMsg,
        events: Vec<CosmosEvent>,
    },
    TimeJumpSeconds {
        msg_id: u64,
        msg_elapsed: f64,
        seconds: i64,
    },
}

pub struct ActionContext<'a, R, L, O> {
    pub market_type: MarketType,
    pub market_config: &'a MarketConfig,
    pub market_collateral_token: Token,
    pub bridge: &'a Bridge,
    pub rng: &'a mut R,
    pub get_open_positions: O,
    pub on_log: L,
}

impl<'a, R, L, O> ActionContext<'a, R, L, O>
where
    R: Rng,
    O: Fn() -> Vec<PositionId>,
    L: Fn(ActionLog),
{
    fn on_log_exec(&self, exec: ExecuteMsg, resp: BridgeResponse<Vec<CosmosEvent>>) {
        (self.on_log)(ActionLog::Execute {
            exec,
            msg_id: resp.msg_id,
            msg_elapsed: resp.msg_elapsed,
            events: resp.data,
        });
    }

    fn on_log_time(&self, resp: BridgeResponse<i64>) {
        (self.on_log)(ActionLog::TimeJumpSeconds {
            msg_id: resp.msg_id,
            msg_elapsed: resp.msg_elapsed,
            seconds: resp.data,
        });
    }

    pub async fn do_action(&mut self, action: Option<Action>, crank_first: bool) -> Result<()> {
        if crank_first {
            loop {
                let resp = self
                    .bridge
                    .query_market::<StatusResp>(QueryMsg::Status { price: None })
                    .await?;
                if resp.data.next_crank.is_some() {
                    let resp = self.bridge.crank().await?;
                    self.on_log_exec(
                        ExecuteMsg::Crank {
                            execs: None,
                            rewards: None,
                        },
                        resp,
                    );
                } else {
                    break;
                }
            }
        }

        if let Some(action) = action {
            match action {
                Action::OpenPosition => {
                    self.open_position().await?;
                }
                Action::UpdatePosition => {
                    self.update_position().await?;
                }
                Action::ClosePosition => {
                    self.close_position().await?;
                }
                Action::SetPrice => {
                    self.set_price().await?;
                }
                Action::TimeJump => {
                    self.time_jump().await?;
                }
            };
        }

        Ok(())
    }

    async fn open_position(&mut self) -> Result<()> {
        let price_point = self.query_price().await?;

        let direction = if self.rand_bool() {
            DirectionToBase::Long
        } else {
            DirectionToBase::Short
        };

        let leverage = self.rand_leverage(direction, self.market_type);
        let max_gains =
            self.rand_max_gains(direction, leverage, self.market_type, &self.market_config);

        let min_collateral: f64 = price_point
            .usd_to_collateral(self.market_config.minimum_deposit_usd)
            .to_string()
            .parse()
            .unwrap();

        let collateral = self.rand_nonzero_collateral(min_collateral..100.0f64);

        let execute_msg = ExecuteMsg::OpenPosition {
            slippage_assert: None,
            leverage,
            direction,
            max_gains,
            stop_loss_override: None,
            take_profit_override: None,
        };

        self.bridge
            .mint_collateral(collateral.into_number_gt_zero())
            .await?;
        self.bridge
            .mint_and_deposit_lp(
                NumberGtZero::new(collateral.into_decimal256() * leverage.into_decimal256())
                    .unwrap(),
            )
            .await?;

        let resp = self
            .bridge
            .exec_market(execute_msg.clone(), Some(collateral.into_number_gt_zero()))
            .await?;
        self.on_log_exec(execute_msg, resp);

        Ok(())
    }

    async fn update_position(&mut self) -> Result<()> {
        let mut ids = (self.get_open_positions)();

        let pos = match ids.choose(self.rng).cloned() {
            None => None,
            Some(pos_id) => self.query_position(pos_id).await?,
        };

        if let Some(pos) = pos {
            let price_point = self.query_price().await?;

            let min_collateral: f64 = price_point
                .usd_to_collateral(self.market_config.minimum_deposit_usd)
                .to_string()
                .parse()
                .unwrap();

            let active_collateral = pos.active_collateral.to_string().parse::<f64>()?;
            let min_remove = 0.00001f64;
            let max_remove = (active_collateral - min_collateral).max(0.0f64);
            // if we can't legitimately remove more collateral, don't even try
            let max_update_option = if max_remove > min_remove { 5 } else { 3 };

            let (execute_msg, funds) = match self.rng.gen_range(0..=max_update_option) {
                0 => {
                    let execute_msg =
                        ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { id: pos.id };
                    let funds = self.rand_number(min_collateral..=100.0f64);
                    (execute_msg, Some(funds))
                }
                1 => {
                    let execute_msg = ExecuteMsg::UpdatePositionAddCollateralImpactSize {
                        id: pos.id,
                        slippage_assert: None,
                    };
                    let funds = self.rand_number(min_collateral..=100.0f64);
                    (execute_msg, Some(funds))
                }

                2 => {
                    let execute_msg = ExecuteMsg::UpdatePositionLeverage {
                        id: pos.id,
                        leverage: self.rand_leverage(pos.direction_to_base, self.market_type),
                        slippage_assert: None,
                    };
                    (execute_msg, None)
                }

                3 => {
                    let execute_msg = ExecuteMsg::UpdatePositionMaxGains {
                        id: pos.id,
                        max_gains: self.rand_max_gains(
                            pos.direction_to_base,
                            pos.leverage,
                            self.market_type,
                            &self.market_config,
                        ),
                    };
                    (execute_msg, None)
                }

                4 => {
                    let execute_msg = ExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage {
                        id: pos.id,
                        amount: self.rand_nonzero_collateral(min_remove..max_remove),
                    };
                    (execute_msg, None)
                }

                5 => {
                    let execute_msg = ExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                        id: pos.id,
                        amount: self.rand_nonzero_collateral(min_remove..max_remove),
                        slippage_assert: None,
                    };
                    (execute_msg, None)
                }
                _ => unreachable!(),
            };

            if let Some(funds) = funds {
                self.bridge
                    .mint_collateral(funds.try_into_non_zero().unwrap())
                    .await?;
                self.bridge
                    .mint_and_deposit_lp(
                        (funds * pos.leverage.into_number())
                            .try_into_non_zero()
                            .unwrap(),
                    )
                    .await?;
            }

            let resp = self
                .bridge
                .exec_market(
                    execute_msg.clone(),
                    funds.map(|funds| funds.to_string().parse()).transpose()?,
                )
                .await?;

            self.on_log_exec(execute_msg, resp);
        }

        Ok(())
    }

    async fn close_position(&mut self) -> Result<()> {
        let mut ids = (self.get_open_positions)();

        if let Some(id) = ids.choose(self.rng).cloned() {
            let execute_msg = ExecuteMsg::ClosePosition {
                id,
                slippage_assert: None,
            };
            let resp = self.bridge.exec_market(execute_msg.clone(), None).await?;
            self.on_log_exec(execute_msg, resp);
        }

        Ok(())
    }

    async fn set_price(&mut self) -> Result<()> {
        let old_price = self.query_price().await?;

        let old_price: f64 = old_price
            .price_base
            .into_number()
            .to_string()
            .parse()
            .unwrap_ext();

        let log_price = old_price.log(2.0);
        let log_price = log_price + self.rng.gen_range(-0.1..=0.1);
        let price = 2.0f64.powf(log_price);

        let price =
            PriceBaseInQuote::try_from_number(price.to_string().parse().unwrap_ext()).unwrap_ext();
        //let price: PriceBaseInQuote = self.rand_number(0.3..5.0f64).to_string().parse()?;
        let execute_msg = ExecuteMsg::SetManualPrice {
            price,
            price_usd: PriceCollateralInUsd::one(),
        };
        let resp = self.bridge.exec_market(execute_msg.clone(), None).await?;
        self.on_log_exec(execute_msg, resp);
        Ok(())
    }

    async fn time_jump(&mut self) -> Result<()> {
        let seconds: i64 = self.rng.gen_range(
            (self.market_config.liquifunding_delay_seconds / 2) as i64
                ..(self.market_config.liquifunding_delay_seconds * 2) as i64,
        );
        let resp = self.bridge.time_jump(seconds).await?;

        self.on_log_time(resp);
        Ok(())
    }

    // market helpers
    async fn query_price(&self) -> Result<PricePoint> {
        let msg = QueryMsg::SpotPrice { timestamp: None };
        let resp = self.bridge.query_market(msg).await?;

        Ok(resp.data)
    }
    async fn query_position(&self, pos_id: PositionId) -> Result<Option<PositionQueryResponse>> {
        let msg = QueryMsg::Positions {
            position_ids: vec![pos_id],
            skip_calc_pending_fees: Some(false),
            fees: None,
            price: None,
        };
        let mut resp = self.bridge.query_market::<PositionsResp>(msg).await?;

        Ok(resp.data.positions.pop())
    }

    // rand helpers
    fn rand_bool(&mut self) -> bool {
        self.rng.gen()
    }

    fn rand_number(&mut self, range: impl SampleRange<f64>) -> Number {
        let value: f64 = self.rng.gen_range(range);
        value.to_string().parse().unwrap()
    }

    fn rand_nonzero_collateral(&mut self, range: impl SampleRange<f64>) -> NonZero<Collateral> {
        let value: f64 = self.rng.gen_range(range);
        let value_decimal256: Decimal256 = value.to_string().parse().unwrap_ext();

        let value_128 = self
            .market_collateral_token
            .into_u128(value_decimal256)
            .unwrap_ext()
            .unwrap_ext();
        let value_truncated = self
            .market_collateral_token
            .from_u128(value_128)
            .unwrap_ext();

        NonZero::new(Collateral::from_decimal256(value_truncated)).unwrap_ext()
    }

    fn rand_leverage(
        &mut self,
        direction: DirectionToBase,
        market_type: MarketType,
    ) -> LeverageToBase {
        let range: RangeInclusive<f64> = match self.market_type {
            MarketType::CollateralIsQuote => 0.25f64..=30.0f64,
            MarketType::CollateralIsBase => match direction {
                DirectionToBase::Long => 1.25f64..=30.0f64,
                DirectionToBase::Short => 0.25f64..=30.0f64,
            },
        };

        self.rand_number(range).to_string().parse().unwrap()
    }

    fn rand_max_gains(
        &mut self,
        direction: DirectionToBase,
        leverage_base: LeverageToBase,
        market_type: MarketType,
        config: &MarketConfig,
    ) -> MaxGainsInQuote {
        fn calculate_max_gains_range_collateral_is_base(
            max_leverage_base: LeverageToBase,
            leverage: LeverageToBase,
            direction_to_base: DirectionToBase,
        ) -> RangeInclusive<f32> {
            let max_leverage_base: f32 = max_leverage_base.to_string().parse().unwrap();
            let leverage: f32 = leverage.to_string().parse().unwrap();
            let direction = if direction_to_base == DirectionToBase::Long {
                1.0
            } else {
                -1.0
            };

            let min = -(1.0f32
                .div(1.0.sub(direction.mul(max_leverage_base)))
                .mul(leverage)
                .mul(direction));

            let max = match direction_to_base {
                DirectionToBase::Long => {
                    -(1.0
                        .div(1.0.sub(direction.div(0.9)))
                        .mul(leverage)
                        .mul(direction))
                }
                DirectionToBase::Short => -(0.5.mul(leverage).mul(direction)),
            };

            min..=max
        }

        let max_leverage_base =
            LeverageToBase::try_from(config.max_leverage.to_string().as_str()).unwrap();

        let leverage_notional: f32 = leverage_base
            .into_signed(direction)
            .into_notional(market_type)
            .into_number()
            .to_string()
            .parse::<f32>()
            .unwrap()
            .abs();

        let max_leverage_notional: f32 = max_leverage_base
            .into_signed(direction)
            .into_notional(market_type)
            .into_number()
            .to_string()
            .parse::<f32>()
            .unwrap()
            .abs();

        let max_gains_can_be_infinite =
            direction == DirectionToBase::Long && market_type == MarketType::CollateralIsBase;

        let finite = {
            let range: RangeInclusive<f32> = match market_type {
                MarketType::CollateralIsQuote => match direction {
                    DirectionToBase::Long => {
                        let min = leverage_notional / max_leverage_notional;
                        let max = leverage_notional;
                        min..=max
                    }
                    DirectionToBase::Short => {
                        let min = leverage_notional / max_leverage_notional;
                        let max = leverage_notional;
                        min..=max
                    }
                },
                MarketType::CollateralIsBase => calculate_max_gains_range_collateral_is_base(
                    max_leverage_base,
                    leverage_base,
                    direction,
                ),
            };

            let value = self.rng.gen_range(range);

            MaxGainsInQuote::Finite(
                value
                    .to_string()
                    .parse()
                    .unwrap_or_else(|_| panic!("{value} should be a valid max gains!")),
            )
        };

        if max_gains_can_be_infinite {
            if self.rng.gen::<bool>() {
                MaxGainsInQuote::PosInfinity
            } else {
                finite
            }
        } else {
            finite
        }
    }
}
