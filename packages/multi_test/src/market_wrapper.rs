/*
    High-level concepts:

    1. All executions that go through the market jump to the next block after
    2. All time jumps move the block height as well

    The basic idea is that it simulates real-world usage
    tests which require manipulating the underlying machinery at a lower level
    must do so via the app, not market
*/

use super::PerpsApp;
use crate::config::{SpotPriceKind, TokenKind, DEFAULT_MARKET, TEST_CONFIG};
use crate::response::CosmosResponseExt;
use crate::time::{BlockInfoChange, TimeJump};
use anyhow::Context;
pub use anyhow::{anyhow, Result};
use cosmwasm_std::{
    to_json_binary, to_json_vec, Addr, Binary, ContractResult, CosmosMsg, Empty, QueryRequest,
    StdError, SystemResult, Uint128, WasmMsg, WasmQuery,
};
use cw_multi_test::{AppResponse, BankSudo, Executor, SudoMsg};
use msg::bridge::{ClientToBridgeMsg, ClientToBridgeWrapper};
use msg::contracts::copy_trading::{
    Config as CopyTradingConfig, ExecuteMsg as CopyTradingExecuteMsg,
    QueryMsg as CopyTradingQueryMsg,
};
use msg::contracts::countertrade::{
    Config as CountertradeConfig, ExecuteMsg as CountertradeExecuteMsg, HasWorkResp,
    QueryMsg as CountertradeQueryMsg,
};
use msg::contracts::cw20::entry::{
    BalanceResponse, ExecuteMsg as Cw20ExecuteMsg, QueryMsg as Cw20QueryMsg, TokenInfoResponse,
};
use msg::contracts::factory::entry::{
    CopyTradingResp, ExecuteMsg as FactoryExecuteMsg, GetReferrerResp, ListRefereeCountResp,
    ListRefereesResp, MarketInfoResponse, QueryMsg as FactoryQueryMsg, RefereeCount,
    ShutdownStatus,
};
use msg::contracts::liquidity_token::LiquidityTokenKind;
use msg::contracts::market::crank::CrankWorkInfo;
use msg::contracts::market::deferred_execution::{
    DeferredExecExecutedEvent, DeferredExecId, DeferredExecQueuedEvent, DeferredExecStatus,
    DeferredExecWithStatus, GetDeferredExecResp, ListDeferredExecsResp,
};
use msg::contracts::market::entry::{
    ClosedPositionCursor, ClosedPositionsResp, DeltaNeutralityFeeResp, ExecuteMsg, Fees,
    InitialPrice, LimitOrderHistoryResp, LimitOrderResp, LimitOrdersResp, LpAction,
    LpActionHistoryResp, LpInfoResp, NewCopyTradingParams, PositionActionHistoryResp,
    PositionsQueryFeeApproach, PriceForQuery, PriceWouldTriggerResp, QueryMsg, ReferralStatsResp,
    SlippageAssert, SpotPriceHistoryResp, StatusResp, StopLoss, TradeHistorySummary,
    TraderActionHistoryResp,
};
use msg::contracts::market::position::{ClosedPosition, PositionsResp};
use msg::contracts::market::spot_price::{
    SpotPriceConfig, SpotPriceConfigInit, SpotPriceFeedDataInit, SpotPriceFeedInit,
};
use msg::contracts::market::{
    config::{Config, ConfigUpdate},
    entry::{
        ExecuteMsg as MarketExecuteMsg, ExecuteOwnerMsg as MarketExecuteOwnerMsg,
        QueryMsg as MarketQueryMsg,
    },
    liquidity::LiquidityStats,
    position::{PositionId, PositionQueryResponse},
};
use msg::contracts::position_token::{
    entry::{
        ExecuteMsg as Cw721ExecuteMsg, NftInfoResponse, OwnerOfResponse, QueryMsg as Cw721QueryMsg,
        TokensResponse,
    },
    Metadata as Cw721Metadata,
};
use msg::prelude::*;
use msg::shared::compat::BackwardsCompatTakeProfit;

use crate::simple_oracle::ExecuteMsg as SimpleOracleExecuteMsg;
use msg::constants::event_key;
use msg::contracts::market::order::OrderId;
use msg::shared::cosmwasm::OrderInMessage;
use msg::shutdown::{ShutdownEffect, ShutdownImpact};
use msg::token::{Token, TokenInit};
use rand::rngs::ThreadRng;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;

pub struct PerpsMarket {
    // we can have multiple markets per app instance
    // PerpsApp is not thread-safe, however (i.e. it's RefCell not Mutex here)
    app: Rc<RefCell<PerpsApp>>,
    pub token: Token,
    pub id: MarketId,
    pub addr: Addr,
    pub copy_trading_addr: Addr,
    /// When enabled, time will jump by one block on every exec
    pub automatic_time_jump_enabled: bool,

    /// Temp for printf debugging / migration
    /// TODO: remove
    pub debug_001: bool,
}

impl PerpsMarket {
    pub fn new(app: Rc<RefCell<PerpsApp>>) -> Result<Self> {
        Self::new_with_type(
            app,
            DEFAULT_MARKET.collateral_type,
            true,
            DEFAULT_MARKET.spot_price,
        )
    }

    pub fn new_with_type(
        app: Rc<RefCell<PerpsApp>>,
        market_type: MarketType,
        bootstap_lp: bool,
        spot_price_kind: SpotPriceKind,
    ) -> Result<Self> {
        let token_init = match DEFAULT_MARKET.token_kind {
            TokenKind::Native => TokenInit::Native {
                denom: TEST_CONFIG.native_denom.to_string(),
                decimal_places: 6,
            },
            TokenKind::Cw20 => {
                let addr = app
                    .borrow_mut()
                    .get_cw20_addr(&DEFAULT_MARKET.cw20_symbol)?;
                TokenInit::Cw20 { addr: addr.into() }
            }
        };
        Self::new_custom(
            app,
            MarketId::new(
                DEFAULT_MARKET.base.clone(),
                DEFAULT_MARKET.quote.clone(),
                market_type,
            ),
            token_init,
            DEFAULT_MARKET.initial_price,
            None,
            None,
            bootstap_lp,
            spot_price_kind,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_custom(
        app: Rc<RefCell<PerpsApp>>,
        id: MarketId,
        token_init: TokenInit,
        initial_price: PriceBaseInQuote,
        initial_price_usd: Option<PriceCollateralInUsd>,
        initial_price_publish_time: Option<Timestamp>,
        bootstap_lp: bool,
        spot_price_kind: SpotPriceKind,
    ) -> Result<Self> {
        // for oracles, set the initial price on the oracle
        // and the market contract gets no initial price (it queries the oracle)
        // for manual, set the initial price on the market contract
        let initial_price = match spot_price_kind {
            SpotPriceKind::Oracle => {
                let mut app = app.borrow_mut();
                let price_publish_time = initial_price_publish_time
                    .map(|x| x.into())
                    .unwrap_or(app.block_info().time);
                let contract_addr = app.simple_oracle_addr.clone();
                app.execute_contract(
                    Addr::unchecked(&TEST_CONFIG.protocol_owner),
                    contract_addr,
                    &SimpleOracleExecuteMsg::SetPrice {
                        value: initial_price.into_number().abs_unsigned(),
                        timestamp: Some(price_publish_time),
                    },
                    &[],
                )?;

                let contract_addr = app.simple_oracle_usd_addr.clone();
                app.execute_contract(
                    Addr::unchecked(&TEST_CONFIG.protocol_owner),
                    contract_addr,
                    &SimpleOracleExecuteMsg::SetPrice {
                        value: initial_price_usd
                            .unwrap_or_else(|| {
                                PriceCollateralInUsd::from_non_zero(initial_price.into_non_zero())
                            })
                            .into_number()
                            .abs_unsigned(),
                        timestamp: Some(price_publish_time),
                    },
                    &[],
                )?;

                None
            }
            SpotPriceKind::Manual => Some(InitialPrice {
                price: initial_price,
                price_usd: initial_price_usd.unwrap_or_else(|| {
                    PriceCollateralInUsd::from_non_zero(initial_price.into_non_zero())
                }),
            }),
        };

        let factory_addr = app.borrow().factory_addr.clone();
        let protocol_owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);

        let copy_trading_msg = msg::contracts::factory::entry::ExecuteMsg::AddCopyTrading {
            new_copy_trading: NewCopyTradingParams {
                name: "Multi test copy trading pool #1".to_owned(),
                description: "Multi test copy trading description".to_owned(),
            },
        };

        let copy_trading_addr = app
            .borrow_mut()
            .execute_contract(
                protocol_owner.clone(),
                factory_addr.clone(),
                &copy_trading_msg,
                &[],
            )?
            .events
            .iter()
            .find(|e| e.ty == "instantiate")
            .context("could not instantiate")?
            .attributes
            .iter()
            .find(|a| a.key == "_contract_address")
            .context("could not find contract_address")?
            .value
            .clone();

        let copy_trading_addr = Addr::unchecked(copy_trading_addr);

        let market_msg = msg::contracts::factory::entry::ExecuteMsg::AddMarket {
            new_market: msg::contracts::market::entry::NewMarketParams {
                market_id: id.clone(),
                token: token_init,
                config: Some(ConfigUpdate {
                    // Many of the tests, especially precise value tests, do not
                    // account for a potential crank fee. Therefore, we disable
                    // it by default and only turn on the crank fee when we're
                    // testing specifically for it.
                    crank_fee_charged: Some(Usd::zero()),
                    crank_fee_reward: Some(Usd::zero()),
                    crank_fee_surcharge: Some(Usd::zero()),
                    // Easier to just go back to the original default than update tests
                    unstake_period_seconds: Some(60 * 60 * 24 * 21),
                    // Same: original default to fix tests
                    trading_fee_notional_size: Some("0.0005".parse().unwrap()),
                    trading_fee_counter_collateral: Some("0.0005".parse().unwrap()),
                    liquidity_cooldown_seconds: Some(0),
                    ..Default::default()
                }),
                spot_price: match spot_price_kind {
                    SpotPriceKind::Manual => SpotPriceConfigInit::Manual {
                        admin: TEST_CONFIG.manual_price_owner.as_str().into(),
                    },
                    SpotPriceKind::Oracle => {
                        let contract_addr = RawAddr::from(app.borrow().simple_oracle_addr.as_ref());
                        let contract_addr_usd =
                            RawAddr::from(app.borrow().simple_oracle_usd_addr.as_ref());
                        SpotPriceConfigInit::Oracle {
                            pyth: None,
                            stride: None,
                            feeds: vec![SpotPriceFeedInit {
                                data: SpotPriceFeedDataInit::Simple {
                                    contract: contract_addr.clone(),
                                    age_tolerance_seconds: 120,
                                },
                                inverted: false,
                                volatile: None,
                            }],
                            feeds_usd: vec![SpotPriceFeedInit {
                                data: SpotPriceFeedDataInit::Simple {
                                    contract: contract_addr_usd,
                                    age_tolerance_seconds: 120,
                                },
                                inverted: false,
                                volatile: None,
                            }],
                            volatile_diff_seconds: None,
                        }
                    }
                },
                initial_borrow_fee_rate: "0.01".parse().unwrap(),
                initial_price,
            },
        };

        let market_addr = app
            .borrow_mut()
            .execute_contract(
                protocol_owner.clone(),
                factory_addr.clone(),
                &market_msg,
                &[],
            )?
            .events
            .iter()
            .find(|e| e.ty == "instantiate")
            .context("could not instantiate")?
            .attributes
            .iter()
            .find(|a| a.key == "_contract_address")
            .context("could not find contract_address")?
            .value
            .clone();

        let market_addr = Addr::unchecked(market_addr);

        let token = app
            .borrow()
            .wrap()
            .query_wasm_smart::<StatusResp>(
                market_addr.clone(),
                &MarketQueryMsg::Status { price: None },
            )?
            .collateral;

        let mut _self = Self {
            app,
            id,
            token,
            addr: market_addr,
            automatic_time_jump_enabled: true,
            debug_001: false,
            copy_trading_addr,
        };

        if bootstap_lp {
            if spot_price_kind == SpotPriceKind::Oracle {
                // not required for manual prices which append on init
                // (technically, it's a "initial_price.is_some()" check, but this is only allowed for manual prices)
                _self.exec_crank_n(&Addr::unchecked("init-cranker"), 1)?;
            }

            // do not go through app get_user, since we want to _always_ bootstrap this user
            _self.exec_mint_and_deposit_liquidity(
                &DEFAULT_MARKET.bootstrap_lp_addr,
                DEFAULT_MARKET.bootstrap_lp_deposit,
            )?;
        }

        Ok(_self)
    }

    // underlying app - private access only
    fn app(&self) -> RefMut<PerpsApp> {
        self.app.borrow_mut()
    }

    pub fn with_rng<A>(&self, f: impl FnOnce(&mut ThreadRng) -> A) -> A {
        f(&mut self.app().rng)
    }

    pub fn set_log_block_time_changes(&self, flag: bool) {
        self.app().log_block_time_changes = flag;
    }

    pub fn now(&self) -> Timestamp {
        self.app().block_info().time.into()
    }

    pub fn clone_trader(&self, index: usize) -> Result<Addr> {
        let (addr, _) = self.app().get_user(
            &format!("trader-{}", index),
            &self.token,
            TEST_CONFIG.new_user_funds,
        )?;
        Ok(addr)
    }

    pub fn clone_lp(&self, index: usize) -> Result<Addr> {
        let (addr, _) = self.app().get_user(
            &format!("lp-{}", index),
            &self.token,
            TEST_CONFIG.new_user_funds,
        )?;
        Ok(addr)
    }

    pub fn set_time(&self, time_jump: TimeJump) -> Result<()> {
        let config = self.query_config()?;
        let block_info_change = BlockInfoChange::from_time_jump(
            time_jump,
            self.app().block_info(),
            config.liquifunding_delay_seconds as u64,
        );
        self.app().set_block_info(block_info_change);

        Ok(())
    }

    pub fn exec_mint_tokens(&self, addr: &Addr, amount: Number) -> Result<AppResponse> {
        self.app().mint_token(
            addr,
            &self.token,
            NonZero::try_from_number(amount).context("Negative or zero token amount")?,
        )
    }

    /// Mint some native coins of the given denom
    pub fn exec_mint_native(
        &self,
        recipient: &Addr,
        denom: impl Into<String>,
        amount: impl Into<Uint128>,
    ) -> Result<AppResponse> {
        self.app().sudo(SudoMsg::Bank(BankSudo::Mint {
            to_address: recipient.to_string(),
            amount: vec![cosmwasm_std::Coin {
                denom: denom.into(),
                amount: amount.into(),
            }],
        }))
    }

    pub fn query_collateral_balance(&self, user_addr: &Addr) -> Result<Number> {
        self.token
            .query_balance(&self.app().querier(), user_addr)
            .map(|x| x.into_number())
    }

    // generic contract methods
    pub fn query<T: DeserializeOwned>(&self, msg: &MarketQueryMsg) -> Result<T> {
        let market_addr = self.addr.clone();

        self.app()
            .wrap()
            .query_wasm_smart(market_addr, &msg)
            .map_err(|err| err.into())
    }

    pub fn raw_query(&self, msg: &MarketQueryMsg) -> Result<Binary> {
        let market_addr = self.addr.clone();

        let request: QueryRequest<Empty> = WasmQuery::Smart {
            contract_addr: market_addr.into(),
            msg: to_json_binary(msg)?,
        }
        .into();

        let raw = to_json_vec(&request).map_err(|serialize_err| {
            StdError::generic_err(format!("Serializing QueryRequest: {}", serialize_err))
        })?;
        match self.app().wrap().raw_query(&raw) {
            SystemResult::Err(system_err) => Err(system_err.into()),
            SystemResult::Ok(ContractResult::Err(contract_err)) => Err(anyhow!("{}", contract_err)),
            SystemResult::Ok(ContractResult::Ok(value)) => Ok(value),
        }
    }

    pub fn exec(&self, sender: &Addr, msg: &MarketExecuteMsg) -> Result<AppResponse> {
        self.exec_funds(sender, msg, Number::ZERO)
    }

    pub fn exec_defer(&self, sender: &Addr, msg: &MarketExecuteMsg) -> Result<DeferResponse> {
        self.exec_defer_wasm_msg(
            sender,
            WasmMsg::Execute {
                contract_addr: self.addr.to_string(),
                msg: to_json_binary(&msg)?,
                funds: Vec::new(),
            },
        )
    }

    /// Like [Self::exec_defer], but attach a crank fee.
    pub fn exec_defer_with_crank_fee(
        &self,
        sender: &Addr,
        msg: &MarketExecuteMsg,
    ) -> Result<DeferResponse> {
        let config = self.query_config()?;
        let price = self.query_current_price()?;
        let crank_fee = price.usd_to_collateral(config.crank_fee_charged);
        let msg = self
            .token
            .into_market_execute_msg(&self.addr, crank_fee, msg.clone())?;
        self.exec_defer_wasm_msg(sender, msg)
    }

    pub fn make_market_msg_with_funds(
        &self,
        msg: &MarketExecuteMsg,
        amount: Number,
    ) -> Result<WasmMsg> {
        self.make_msg_with_funds(msg, amount, &self.addr)
    }

    fn make_msg_with_funds<T: Serialize + Clone + std::fmt::Debug>(
        &self,
        msg: &T,
        amount: Number,
        contract: &Addr,
    ) -> Result<WasmMsg> {
        let amount = Collateral::from_decimal256(
            amount
                .try_into_non_negative_value()
                .context("funds must be positive!")?,
        );

        Ok(match NonZero::new(amount) {
            None => WasmMsg::Execute {
                contract_addr: contract.to_string(),
                msg: to_json_binary(msg)?,
                funds: vec![],
            },
            Some(amount) => self.token.into_execute_msg(contract, amount.raw(), &msg)?,
        })
    }

    pub fn exec_funds(
        &self,
        sender: &Addr,
        msg: &MarketExecuteMsg,
        amount: Number,
    ) -> Result<AppResponse> {
        let wasm_msg = self.make_market_msg_with_funds(msg, amount)?;
        self.exec_wasm_msg(sender, wasm_msg)
    }

    pub fn exec_wasm_msg(&self, sender: &Addr, msg: WasmMsg) -> Result<AppResponse> {
        let cosmos_msg = CosmosMsg::Wasm(msg);
        let res = self.app().execute(sender.clone(), cosmos_msg)?;

        if self.automatic_time_jump_enabled {
            self.set_time(TimeJump::Blocks(1))?;
        }

        Ok(res)
    }

    // market queries
    pub fn query_status(&self) -> Result<StatusResp> {
        self.query(&MarketQueryMsg::Status { price: None })
    }

    // market queries
    pub fn query_status_with_price(&self, price: PriceForQuery) -> Result<StatusResp> {
        self.query(&MarketQueryMsg::Status { price: Some(price) })
    }

    pub fn query_crank_stats(&self) -> Result<Option<CrankWorkInfo>> {
        self.query_status().map(|x| x.next_crank)
    }

    pub fn query_position(&self, position_id: PositionId) -> Result<PositionQueryResponse> {
        let PositionsResp {
            mut positions,
            pending_close,
            closed,
        } = self.query(&MarketQueryMsg::Positions {
            position_ids: vec![position_id],
            // Backwards compat in the tests
            skip_calc_pending_fees: Some(true),
            fees: None,
            price: None,
        })?;
        anyhow::ensure!(pending_close.is_empty());
        anyhow::ensure!(closed.is_empty());
        positions.pop().ok_or_else(|| anyhow!("no positions"))
    }

    pub fn query_position_with_price(
        &self,
        position_id: PositionId,
        price: PriceForQuery,
    ) -> Result<PositionQueryResponse> {
        let PositionsResp {
            mut positions,
            pending_close,
            closed,
        } = self.query(&MarketQueryMsg::Positions {
            position_ids: vec![position_id],
            // Backwards compat in the tests
            skip_calc_pending_fees: Some(true),
            fees: None,
            price: Some(price),
        })?;
        anyhow::ensure!(pending_close.is_empty());
        anyhow::ensure!(closed.is_empty());
        positions.pop().ok_or_else(|| anyhow!("no positions"))
    }

    pub fn query_position_pending_close_with_price(
        &self,
        position_id: PositionId,
        price: PriceForQuery,
    ) -> Result<ClosedPosition> {
        let PositionsResp {
            positions,
            mut pending_close,
            closed,
        } = self.query(&MarketQueryMsg::Positions {
            position_ids: vec![position_id],
            // Backwards compat in the tests
            skip_calc_pending_fees: Some(true),
            fees: None,
            price: Some(price),
        })?;
        anyhow::ensure!(positions.is_empty());
        anyhow::ensure!(closed.is_empty());
        pending_close.pop().ok_or_else(|| anyhow!("no positions"))
    }

    pub fn query_price_would_trigger(&self, price: PriceBaseInQuote) -> Result<bool> {
        let PriceWouldTriggerResp { would_trigger } =
            self.query(&MarketQueryMsg::PriceWouldTrigger { price })?;
        Ok(would_trigger)
    }

    pub fn query_position_with_pending_fees(
        &self,
        position_id: PositionId,
        fees: PositionsQueryFeeApproach,
    ) -> Result<PositionQueryResponse> {
        let PositionsResp {
            mut positions,
            pending_close,
            closed,
        } = self.query(&MarketQueryMsg::Positions {
            position_ids: vec![position_id],
            skip_calc_pending_fees: None,
            fees: Some(fees),
            price: None,
        })?;
        anyhow::ensure!(pending_close.is_empty());
        anyhow::ensure!(closed.is_empty());
        positions.pop().ok_or_else(|| anyhow!("no positions"))
    }

    pub fn query_position_pending_close(
        &self,
        position_id: PositionId,
        fees: PositionsQueryFeeApproach,
    ) -> Result<ClosedPosition> {
        let PositionsResp {
            positions,
            mut pending_close,
            closed,
        } = self.query(&MarketQueryMsg::Positions {
            position_ids: vec![position_id],
            skip_calc_pending_fees: None,
            fees: Some(fees),
            price: None,
        })?;
        anyhow::ensure!(positions.is_empty());
        anyhow::ensure!(closed.is_empty());
        pending_close
            .pop()
            .ok_or_else(|| anyhow!("no position pending close"))
    }

    /// Only gets open positions.
    pub fn query_positions(&self, owner: &Addr) -> Result<Vec<PositionQueryResponse>> {
        let ids = self
            .query_position_token_ids(owner)?
            .into_iter()
            .map(|id| Ok(id.parse()?))
            .collect::<Result<Vec<PositionId>>>()?;

        let PositionsResp {
            positions,
            pending_close,
            closed,
        } = self.query(&MarketQueryMsg::Positions {
            position_ids: ids,
            skip_calc_pending_fees: Some(true),
            fees: None,
            price: None,
        })?;
        anyhow::ensure!(pending_close.is_empty());
        anyhow::ensure!(closed.is_empty());

        Ok(positions)
    }

    pub fn query_closed_positions(
        &self,
        owner: &Addr,
        cursor: Option<ClosedPositionCursor>,
        limit: Option<u32>,
        order: Option<OrderInMessage>,
    ) -> Result<ClosedPositionsResp> {
        self.query(&MarketQueryMsg::ClosedPositionHistory {
            owner: owner.clone().into(),
            cursor,
            limit,
            order,
        })
    }

    pub fn query_limit_order(&self, order_id: OrderId) -> Result<LimitOrderResp> {
        self.query(&QueryMsg::LimitOrder { order_id })
    }

    pub fn query_limit_orders(
        &self,
        addr: &Addr,
        start_after: Option<OrderId>,
        limit: Option<u32>,
        order: Option<OrderInMessage>,
    ) -> Result<LimitOrdersResp> {
        self.query(&QueryMsg::LimitOrders {
            owner: addr.clone().into(),
            start_after,
            limit,
            order,
        })
    }

    pub fn query_deferred_execs(&self, owner: &Addr) -> Result<Vec<DeferredExecWithStatus>> {
        let mut res = vec![];
        let mut start_after = None;
        loop {
            let ListDeferredExecsResp {
                mut items,
                next_start_after,
            } = self.query(&QueryMsg::ListDeferredExecs {
                addr: owner.clone().into(),
                start_after: start_after.take(),
                limit: None,
            })?;
            res.append(&mut items);
            match next_start_after {
                None => break Ok(res),
                Some(next_start_after) => start_after = Some(next_start_after),
            }
        }
    }

    pub fn query_current_price(&self) -> Result<PricePoint> {
        self.query(&MarketQueryMsg::SpotPrice { timestamp: None })
    }

    pub fn query_closed_position(
        &self,
        owner: &Addr,
        pos_id: PositionId,
    ) -> Result<ClosedPosition> {
        let PositionsResp {
            positions,
            pending_close,
            mut closed,
        } = self.query(&MarketQueryMsg::Positions {
            position_ids: vec![pos_id],
            skip_calc_pending_fees: Some(true),
            fees: None,
            price: None,
        })?;
        anyhow::ensure!(positions.is_empty());
        anyhow::ensure!(pending_close.is_empty());
        anyhow::ensure!(closed.len() == 1);
        let closed1 = closed.pop().unwrap();

        let closed2 = self
            .query_closed_positions(owner, None, None, None)?
            .positions
            .into_iter()
            .find(|p| p.id == pos_id)
            .ok_or_else(|| anyhow!("no position"))?;

        anyhow::ensure!(closed1 == closed2);

        Ok(closed1)
    }

    pub fn query_liquidity_stats(&self) -> Result<LiquidityStats> {
        self.query_status().map(|x| x.liquidity)
    }

    pub fn query_config(&self) -> Result<Config> {
        self.query_status().map(|x| x.config)
    }

    pub fn query_fees(&self) -> Result<Fees> {
        self.query_status().map(|x| x.fees)
    }

    pub fn exec_provide_crank_funds(&self, addr: &Addr, amount: Number) -> Result<AppResponse> {
        let msg = self.token.into_market_execute_msg(
            &self.addr,
            Collateral::try_from_number(amount)?,
            MarketExecuteMsg::ProvideCrankFunds {},
        )?;
        self.exec_wasm_msg(addr, msg)
    }

    pub fn query_lp_info(&self, addr: &Addr) -> Result<LpInfoResp> {
        let lp_info_resp: LpInfoResp = self.query(&MarketQueryMsg::LpInfo {
            liquidity_provider: addr.clone().into(),
        })?;
        if let Some(unstaking) = &lp_info_resp.unstaking {
            anyhow::ensure!(
                unstaking.xlp_unstaking.raw()
                    == ((unstaking.collected + unstaking.available).unwrap() + unstaking.pending)
                        .unwrap(),
                "Incoherent unstaking value: {unstaking:?}"
            );
        }
        Ok(lp_info_resp)
    }

    pub fn query_referral_stats(&self, addr: &Addr) -> Result<ReferralStatsResp> {
        self.query(&MarketQueryMsg::ReferralStats { addr: addr.into() })
    }

    pub fn query_trade_history_summary(&self, addr: &Addr) -> Result<TradeHistorySummary> {
        self.query(&MarketQueryMsg::TradeHistorySummary {
            addr: addr.clone().into(),
        })
    }

    pub fn query_position_action_history(
        &self,
        id: PositionId,
    ) -> Result<PositionActionHistoryResp> {
        // NOTE we're not doing any pagination. If you write a test that has
        // more than MAX_LIMIT entries, you'll need to add pagination.
        self.query(&MarketQueryMsg::PositionActionHistory {
            id,
            start_after: None,
            limit: None,
            order: None,
        })
    }

    pub fn query_trader_action_history(&self, owner: &Addr) -> Result<TraderActionHistoryResp> {
        // NOTE we're not doing any pagination. If you write a test that has
        // more than MAX_LIMIT entries, you'll need to add pagination.
        self.query(&MarketQueryMsg::TraderActionHistory {
            owner: owner.into(),
            start_after: None,
            limit: None,
            order: None,
        })
    }

    pub fn query_lp_action_history(&self, addr: &Addr) -> Result<LpActionHistoryResp> {
        // NOTE we're not doing any pagination. If you write a test that has
        // more than MAX_LIMIT entries, you'll need to add pagination.
        self.query(&MarketQueryMsg::LpActionHistory {
            addr: addr.clone().into(),
            start_after: None,
            limit: None,
            order: None,
        })
    }

    pub fn query_lp_action_history_full(
        &self,
        addr: &Addr,
        order: OrderInMessage,
    ) -> Result<Vec<LpAction>> {
        let mut start_after = None;
        const LIMIT: Option<u32> = Some(2);
        let mut res = vec![];

        loop {
            let LpActionHistoryResp {
                mut actions,
                next_start_after,
            } = self.query(&MarketQueryMsg::LpActionHistory {
                addr: addr.clone().into(),
                start_after: start_after.take(),
                limit: LIMIT,
                order: Some(order),
            })?;
            res.append(&mut actions);
            if next_start_after.is_none() {
                break Ok(res);
            }

            start_after = next_start_after;
        }
    }

    pub fn query_limit_order_history(&self, addr: &Addr) -> Result<LimitOrderHistoryResp> {
        // NOTE we're not doing any pagination. If you write a test that has
        // more than MAX_LIMIT entries, you'll need to add pagination.
        self.query(&MarketQueryMsg::LimitOrderHistory {
            addr: addr.clone().into(),
            start_after: None,
            limit: None,
            order: None,
        })
    }

    pub fn query_slippage_fee(
        &self,
        notional_delta: Number,
        pos_delta_neutrality_fee_margin: Option<Collateral>,
    ) -> Result<DeltaNeutralityFeeResp> {
        self.query(&MarketQueryMsg::DeltaNeutralityFee {
            notional_delta: Signed::<Notional>::from_number(notional_delta),
            pos_delta_neutrality_fee_margin,
        })
    }

    pub fn query_spot_price_history(
        &self,
        start_after: Option<Timestamp>,
        limit: Option<u32>,
        order: Option<OrderInMessage>,
    ) -> Result<Vec<PricePoint>> {
        let resp: SpotPriceHistoryResp = self.query(&MarketQueryMsg::SpotPriceHistory {
            start_after,
            limit,
            order,
        })?;

        Ok(resp.price_points)
    }

    // market executions
    pub fn exec_refresh_price(&self) -> Result<PriceResponse> {
        let price_resp = self.query_current_price()?;
        self.exec_set_price(price_resp.price_base)
    }

    pub fn exec_set_price(&self, price: PriceBaseInQuote) -> Result<PriceResponse> {
        self.exec_set_price_with_usd(price, None)
    }

    pub fn exec_set_price_time(
        &self,
        price: PriceBaseInQuote,
        timestamp: Option<Timestamp>,
    ) -> Result<PriceResponse> {
        self.exec_set_price_with_usd_time(price, None, timestamp)
    }

    pub fn exec_set_price_with_usd(
        &self,
        price: PriceBaseInQuote,
        price_usd: Option<PriceCollateralInUsd>,
    ) -> Result<PriceResponse> {
        self.exec_set_price_with_usd_time(price, price_usd, None)
    }

    pub fn exec_set_price_with_usd_time(
        &self,
        price: PriceBaseInQuote,
        price_usd: Option<PriceCollateralInUsd>,
        timestamp: Option<Timestamp>,
    ) -> Result<PriceResponse> {
        let price_usd = price_usd.unwrap_or(
            price
                .try_into_usd(&self.id)
                .unwrap_or(PriceCollateralInUsd::one()),
        );

        match self.query_config()?.spot_price {
            SpotPriceConfig::Manual { admin } => {
                if timestamp.is_some() {
                    anyhow::bail!("Manual price does not support setting timestamp");
                }

                let resp = self.exec(
                    &admin,
                    &MarketExecuteMsg::SetManualPrice { price, price_usd },
                )?;

                Ok(PriceResponse {
                    base: resp.clone(),
                    usd: resp,
                })
            }
            SpotPriceConfig::Oracle { .. } => {
                let timestamp = timestamp.unwrap_or(self.now());
                let base_resp = self.exec_set_oracle_price_base(price, timestamp)?;
                let usd_resp = self.exec_set_oracle_price_usd(price_usd, timestamp)?;
                self.exec_crank_n(&Addr::unchecked(&TEST_CONFIG.protocol_owner), 0)?;

                Ok(PriceResponse {
                    base: base_resp,
                    usd: usd_resp,
                })
            }
        }
    }

    pub fn exec_set_oracle_price_base(
        &self,
        price_base: PriceBaseInQuote,
        timestamp: Timestamp,
    ) -> Result<AppResponse> {
        let contract_addr = self.app().simple_oracle_addr.clone();

        self.app().execute_contract(
            Addr::unchecked(&TEST_CONFIG.protocol_owner),
            contract_addr,
            &SimpleOracleExecuteMsg::SetPrice {
                value: price_base.into_non_zero().into_decimal256(),
                timestamp: Some(timestamp.into()),
            },
            &[],
        )
    }
    pub fn exec_set_oracle_price_usd(
        &self,
        price_usd: PriceCollateralInUsd,
        timestamp: Timestamp,
    ) -> Result<AppResponse> {
        let contract_addr = self.app().simple_oracle_usd_addr.clone();

        self.app().execute_contract(
            Addr::unchecked(&TEST_CONFIG.protocol_owner),
            contract_addr,
            &SimpleOracleExecuteMsg::SetPrice {
                value: price_usd.into_number().abs_unsigned(),
                timestamp: Some(timestamp.into()),
            },
            &[],
        )
    }

    pub fn exec_set_config(&self, config_update: ConfigUpdate) -> Result<AppResponse> {
        self.exec(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &MarketExecuteMsg::Owner(MarketExecuteOwnerMsg::ConfigUpdate {
                update: Box::new(config_update),
            }),
        )
    }
    pub fn exec_crank(&self, sender: &Addr) -> Result<AppResponse> {
        self.exec(
            sender,
            &MarketExecuteMsg::Crank {
                execs: None,
                rewards: None,
            },
        )
    }

    pub fn exec_crank_n(&self, sender: &Addr, n: u32) -> Result<AppResponse> {
        self.exec(
            sender,
            &MarketExecuteMsg::Crank {
                execs: Some(n),
                rewards: None,
            },
        )
    }

    pub fn exec_crank_single(&self, sender: &Addr) -> Result<AppResponse> {
        self.exec_crank_n(sender, 1)
    }

    pub fn exec_crank_till_finished(&self, sender: &Addr) -> Result<Vec<AppResponse>> {
        let mut responses = Vec::new();

        loop {
            let status = self.query_status()?;
            let resp = self.exec(
                sender,
                &MarketExecuteMsg::Crank {
                    execs: None,
                    rewards: None,
                },
            )?;

            responses.push(resp);

            // Only check if there is no work after doing one more crank to
            // make sure that the last ignored "Completed" work item is also done.
            if status.deferred_execution_items == 0 && status.next_crank.is_none() {
                break;
            }
        }

        Ok(responses)
    }

    pub fn exec_crank_till_finished_with_rewards(
        &self,
        sender: &Addr,
        rewards: &Addr,
    ) -> Result<Vec<AppResponse>> {
        let mut responses = Vec::new();

        loop {
            let status = self.query_status()?;
            if status.deferred_execution_items == 0 && status.next_crank.is_none() {
                break;
            }

            let resp = self.exec(
                sender,
                &MarketExecuteMsg::Crank {
                    execs: None,
                    rewards: Some(rewards.clone().into()),
                },
            )?;

            responses.push(resp);
        }

        Ok(responses)
    }

    pub fn exec_deposit_liquidity(&self, addr: &Addr, amount: Number) -> Result<AppResponse> {
        self.exec_funds(
            addr,
            &MarketExecuteMsg::DepositLiquidity {
                stake_to_xlp: false,
            },
            amount,
        )
    }

    pub fn exec_withdraw_liquidity(
        &self,
        addr: &Addr,
        amount: Option<Number>,
    ) -> Result<AppResponse> {
        self.exec(
            addr,
            &MarketExecuteMsg::WithdrawLiquidity {
                lp_amount: match amount {
                    None => None,
                    Some(amount) => Some(
                        NonZero::<LpToken>::try_from_number(amount)
                            .context("exec_withdraw_liquidity")?,
                    ),
                },
            },
        )
    }

    pub fn exec_claim_yield(&self, addr: &Addr) -> Result<AppResponse> {
        self.exec(addr, &MarketExecuteMsg::ClaimYield {})
    }

    pub fn exec_mint_and_deposit_liquidity(
        &self,
        user_addr: &Addr,
        amount: Number,
    ) -> Result<AppResponse> {
        self.exec_mint_tokens(user_addr, amount)?;
        self.exec_funds(
            user_addr,
            &MarketExecuteMsg::DepositLiquidity {
                stake_to_xlp: false,
            },
            amount,
        )
    }

    pub fn exec_mint_and_deposit_liquidity_xlp(
        &self,
        user_addr: &Addr,
        amount: Number,
    ) -> Result<AppResponse> {
        self.exec_mint_tokens(user_addr, amount)?;
        self.exec_funds(
            user_addr,
            &MarketExecuteMsg::DepositLiquidity { stake_to_xlp: true },
            amount,
        )
    }

    pub fn exec_mint_and_deposit_liquidity_full(
        &self,
        user_addr: &Addr,
        amount: Number,
        stake_to_xlp: bool,
    ) -> Result<AppResponse> {
        self.exec_mint_tokens(user_addr, amount)?;
        self.exec_funds(
            user_addr,
            &MarketExecuteMsg::DepositLiquidity { stake_to_xlp },
            amount,
        )
    }

    pub fn exec_reinvest_yield(
        &self,
        user_addr: &Addr,
        amount: Option<NonZero<Collateral>>,
        stake_to_xlp: bool,
    ) -> Result<AppResponse> {
        self.exec(
            user_addr,
            &MarketExecuteMsg::ReinvestYield {
                stake_to_xlp,
                amount,
            },
        )
    }

    pub fn exec_stake_lp(&self, user_addr: &Addr, amount: Option<Number>) -> Result<AppResponse> {
        self.exec(
            user_addr,
            &MarketExecuteMsg::StakeLp {
                amount: match amount {
                    None => None,
                    Some(amount) => {
                        Some(NonZero::try_from_number(amount).context("exec_stake_lp")?)
                    }
                },
            },
        )
    }

    pub fn exec_unstake_xlp(
        &self,
        user_addr: &Addr,
        amount: Option<Number>,
    ) -> Result<AppResponse> {
        self.exec(
            user_addr,
            &MarketExecuteMsg::UnstakeXlp {
                amount: match amount {
                    None => None,
                    Some(amount) => {
                        Some(NonZero::try_from_number(amount).context("exec_unstake_xlp")?)
                    }
                },
            },
        )
    }

    pub fn exec_collect_unstaked_lp(&self, user_addr: &Addr) -> Result<AppResponse> {
        self.exec(user_addr, &MarketExecuteMsg::CollectUnstakedLp {})
    }

    pub fn exec_transfer_dao_fees(&self, sender: &Addr) -> Result<AppResponse> {
        self.exec(sender, &MarketExecuteMsg::TransferDaoFees {})
    }

    // Taking TryInto impls allows us to avoid noise in the tests
    // and just use strings as needed, but exec_open_position_raw
    // is available where you have the precise type
    #[allow(clippy::too_many_arguments)]
    pub fn exec_open_position(
        &self,
        sender: &Addr,
        collateral: impl TryInto<NumberGtZero, Error = anyhow::Error>,
        leverage: impl TryInto<LeverageToBase, Error = anyhow::Error>,
        direction: DirectionToBase,
        max_gains: impl TryInto<MaxGainsInQuote, Error = anyhow::Error>,
        slippage_assert: Option<SlippageAssert>,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<(PositionId, DeferResponse)> {
        self.exec_open_position_raw(
            sender,
            collateral.try_into()?.into(),
            slippage_assert,
            leverage.try_into()?,
            direction,
            max_gains.try_into()?,
            stop_loss_override,
            take_profit_override,
        )
    }

    // Taking TryInto impls allows us to avoid noise in the tests
    // and just use strings as needed, but exec_open_position_raw
    // is available where you have the precise type
    #[allow(clippy::too_many_arguments)]
    pub fn exec_open_position_take_profit(
        &self,
        sender: &Addr,
        collateral: impl TryInto<NumberGtZero, Error = anyhow::Error>,
        leverage: impl TryInto<LeverageToBase, Error = anyhow::Error>,
        direction: DirectionToBase,
        slippage_assert: Option<SlippageAssert>,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit: TakeProfitTrader,
    ) -> Result<(PositionId, DeferResponse)> {
        self.exec_open_position_take_profit_raw(
            sender,
            collateral.try_into()?.into(),
            slippage_assert,
            leverage.try_into()?,
            direction,
            stop_loss_override,
            take_profit,
        )
    }

    // Taking TryInto impls allows us to avoid noise in the tests
    // and just use strings as needed, but exec_open_position_raw
    // is available where you have the precise type
    #[allow(clippy::too_many_arguments)]
    pub fn exec_open_position_refresh_price(
        &self,
        sender: &Addr,
        collateral: impl TryInto<NumberGtZero, Error = anyhow::Error>,
        leverage: impl TryInto<LeverageToBase, Error = anyhow::Error>,
        direction: DirectionToBase,
        max_gains: impl TryInto<MaxGainsInQuote, Error = anyhow::Error>,
        slippage_assert: Option<SlippageAssert>,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<(PositionId, DeferResponse)> {
        let queue_res = self.exec_open_position_queue_only(
            sender,
            collateral,
            leverage,
            direction,
            max_gains,
            slippage_assert,
            stop_loss_override,
            take_profit_override,
        )?;

        // the queue above doesn't move forward a block
        // and we need to do that for the price to be valid
        self.set_time(TimeJump::Blocks(1)).unwrap();
        self.exec_refresh_price().unwrap();

        self.exec_open_position_process_queue_response(sender, queue_res, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn exec_open_position_raw(
        &self,
        sender: &Addr,
        collateral: Number,
        slippage_assert: Option<SlippageAssert>,
        leverage: LeverageToBase,
        direction: DirectionToBase,
        max_gains: MaxGainsInQuote,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<(PositionId, DeferResponse)> {
        let queue_resp = self.exec_open_position_raw_queue_only(
            sender,
            collateral,
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
        )?;

        self.exec_open_position_process_queue_response(sender, queue_resp, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn exec_open_position_take_profit_raw(
        &self,
        sender: &Addr,
        collateral: Number,
        slippage_assert: Option<SlippageAssert>,
        leverage: LeverageToBase,
        direction: DirectionToBase,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit: TakeProfitTrader,
    ) -> Result<(PositionId, DeferResponse)> {
        let queue_resp = self.exec_open_position_take_profit_raw_queue_only(
            sender,
            collateral,
            slippage_assert,
            leverage,
            direction,
            stop_loss_override,
            take_profit,
        )?;

        self.exec_open_position_process_queue_response(sender, queue_resp, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn exec_open_position_queue_only(
        &self,
        sender: &Addr,
        collateral: impl TryInto<NumberGtZero, Error = anyhow::Error>,
        leverage: impl TryInto<LeverageToBase, Error = anyhow::Error>,
        direction: DirectionToBase,
        max_gains: impl TryInto<MaxGainsInQuote, Error = anyhow::Error>,
        slippage_assert: Option<SlippageAssert>,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<DeferQueueResponse> {
        self.exec_open_position_raw_queue_only(
            sender,
            collateral.try_into()?.into(),
            slippage_assert,
            leverage.try_into()?,
            direction,
            max_gains.try_into()?,
            stop_loss_override,
            take_profit_override,
        )
    }

    pub fn exec_open_position_process_queue_response(
        &self,
        cranker: &Addr,
        queue_response: DeferQueueResponse,
        crank_execs: Option<u32>,
    ) -> Result<(PositionId, DeferResponse)> {
        // Open position always happens through a deferred exec
        let defer_res = self.exec_defer_queue_process(cranker, queue_response, crank_execs)?;

        let res = defer_res.exec_resp().clone();

        let pos_id = res.event_first_value("position-open", "pos-id")?.parse()?;

        Ok((pos_id, defer_res))
    }

    // this does *not* automatic time jump
    // backwards-compatible for max-gains
    #[allow(clippy::too_many_arguments)]
    pub fn exec_open_position_raw_queue_only(
        &self,
        sender: &Addr,
        collateral: Number,
        slippage_assert: Option<SlippageAssert>,
        leverage: LeverageToBase,
        direction: DirectionToBase,
        max_gains: MaxGainsInQuote,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<DeferQueueResponse> {
        let price = self.query_current_price()?;
        let collateral = Collateral::try_from_number(collateral)?;

        // eh, this is a nice convenience to not have to rewrite all the tests
        // when BackwardsCompatTakeProfit is deprecated from main code, it could be moved entirely into test code
        let take_profit = BackwardsCompatTakeProfit {
            leverage,
            direction,
            collateral: NonZero::new(collateral).unwrap(),
            market_type: self.id.get_market_type(),
            max_gains,
            take_profit: take_profit_override,
            price_point: &price,
        }
        .calc()?;

        let msg = self.token.into_market_execute_msg(
            &self.addr,
            collateral,
            MarketExecuteMsg::OpenPosition {
                slippage_assert,
                leverage,
                direction,
                max_gains: None,
                stop_loss_override,
                take_profit: Some(take_profit),
            },
        )?;

        // Open position always happens through a deferred exec
        self.exec_defer_queue_wasm_msg(sender, msg)
    }

    // this does *not* automatic time jump
    #[allow(clippy::too_many_arguments)]
    pub fn exec_open_position_take_profit_raw_queue_only(
        &self,
        sender: &Addr,
        collateral: Number,
        slippage_assert: Option<SlippageAssert>,
        leverage: LeverageToBase,
        direction: DirectionToBase,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit: TakeProfitTrader,
    ) -> Result<DeferQueueResponse> {
        let collateral = Collateral::try_from_number(collateral)?;

        let msg = self.token.into_market_execute_msg(
            &self.addr,
            collateral,
            MarketExecuteMsg::OpenPosition {
                slippage_assert,
                leverage,
                direction,
                max_gains: None,
                stop_loss_override,
                take_profit: Some(take_profit),
            },
        )?;

        // Open position always happens through a deferred exec
        self.exec_defer_queue_wasm_msg(sender, msg)
    }

    pub fn exec_close_position(
        &self,
        sender: &Addr,
        position_id: PositionId,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<DeferResponse> {
        self.exec_defer(
            sender,
            &MarketExecuteMsg::ClosePosition {
                id: position_id,
                slippage_assert,
            },
        )
    }

    pub fn exec_close_position_refresh_price(
        &self,
        sender: &Addr,
        position_id: PositionId,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<DeferResponse> {
        let queue_res =
            self.exec_close_position_queue_only(sender, position_id, slippage_assert)?;

        self.set_time(TimeJump::Blocks(1)).unwrap();
        self.exec_refresh_price().unwrap();

        self.exec_defer_queue_process(sender, queue_res, None)
    }

    pub fn exec_close_position_queue_only(
        &self,
        sender: &Addr,
        position_id: PositionId,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<DeferQueueResponse> {
        self.exec_defer_queue_wasm_msg(
            sender,
            WasmMsg::Execute {
                contract_addr: self.addr.to_string(),
                msg: to_json_binary(&MarketExecuteMsg::ClosePosition {
                    id: position_id,
                    slippage_assert,
                })?,
                funds: Vec::new(),
            },
        )
    }

    pub fn exec_update_position_collateral_impact_leverage(
        &self,
        sender: &Addr,
        position_id: PositionId,
        collateral_delta: Signed<Collateral>,
    ) -> Result<DeferResponse> {
        let msg = self.token.into_market_execute_msg(
            &self.addr,
            collateral_delta
                .try_into_non_negative_value()
                .unwrap_or_default(),
            if collateral_delta.is_negative() {
                MarketExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage {
                    id: position_id,
                    amount: collateral_delta
                        .abs()
                        .try_into_non_zero()
                        .context("collateral_delta is zero")?,
                }
            } else {
                MarketExecuteMsg::UpdatePositionAddCollateralImpactLeverage { id: position_id }
            },
        )?;

        self.exec_defer_wasm_msg(sender, msg)
    }

    pub fn exec_update_position_collateral_impact_size(
        &self,
        sender: &Addr,
        position_id: PositionId,
        collateral_delta: Signed<Collateral>,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<DeferResponse> {
        let msg = self.token.into_market_execute_msg(
            &self.addr,
            collateral_delta
                .try_into_non_negative_value()
                .unwrap_or_default(),
            if collateral_delta.is_negative() {
                MarketExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                    id: position_id,
                    amount: collateral_delta
                        .abs()
                        .try_into_non_zero()
                        .context("collateral_delta is 0")?,
                    slippage_assert,
                }
            } else {
                MarketExecuteMsg::UpdatePositionAddCollateralImpactSize {
                    id: position_id,
                    slippage_assert,
                }
            },
        )?;

        self.exec_defer_wasm_msg(sender, msg)
    }

    pub fn exec_update_position_leverage(
        &self,
        sender: &Addr,
        position_id: PositionId,
        leverage: LeverageToBase,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<DeferResponse> {
        self.exec_defer(
            sender,
            &MarketExecuteMsg::UpdatePositionLeverage {
                id: position_id,
                leverage,
                slippage_assert,
            },
        )
    }

    // this does *not* automatic time jump
    pub fn exec_update_position_leverage_queue_only(
        &self,
        sender: &Addr,
        position_id: PositionId,
        leverage: LeverageToBase,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<DeferQueueResponse> {
        self.exec_defer_queue_wasm_msg(
            sender,
            WasmMsg::Execute {
                contract_addr: self.addr.to_string(),
                msg: to_json_binary(&MarketExecuteMsg::UpdatePositionLeverage {
                    id: position_id,
                    leverage,
                    slippage_assert,
                })?,
                funds: Vec::new(),
            },
        )
    }

    pub fn exec_update_position_max_gains(
        &self,
        sender: &Addr,
        position_id: PositionId,
        max_gains: MaxGainsInQuote,
    ) -> Result<DeferResponse> {
        // converting to take profit price here, instead of rewriting all the tests, for convenience

        let pos = self.query_position(position_id)?;
        let price_point = self.query_current_price()?;
        let take_profit_price = BackwardsCompatTakeProfit {
            collateral: pos.active_collateral,
            direction: pos.direction_to_base,
            leverage: pos.leverage,
            market_type: self.id.get_market_type(),
            price_point: &price_point,
            max_gains,
            take_profit: None,
        }
        .calc()?;

        self.exec_defer(
            sender,
            &MarketExecuteMsg::UpdatePositionTakeProfitPrice {
                id: position_id,
                price: take_profit_price,
            },
        )
    }

    pub fn exec_update_position_take_profit(
        &self,
        sender: &Addr,
        position_id: PositionId,
        take_profit_price: TakeProfitTrader,
    ) -> Result<DeferResponse> {
        self.exec_defer(
            sender,
            &MarketExecuteMsg::UpdatePositionTakeProfitPrice {
                id: position_id,
                price: take_profit_price,
            },
        )
    }

    pub fn exec_update_position_stop_loss(
        &self,
        sender: &Addr,
        position_id: PositionId,
        stop_loss: StopLoss,
    ) -> Result<DeferResponse> {
        self.exec_defer(
            sender,
            &MarketExecuteMsg::UpdatePositionStopLossPrice {
                id: position_id,
                stop_loss,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn exec_place_limit_order(
        &self,
        sender: &Addr,
        collateral: NonZero<Collateral>,
        trigger_price: PriceBaseInQuote,
        leverage: LeverageToBase,
        direction: DirectionToBase,
        max_gains: MaxGainsInQuote,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<(OrderId, DeferResponse)> {
        // eh, this is a nice convenience to not have to rewrite all the tests
        // when BackwardsCompatTakeProfit is deprecated from main code, it could be moved entirely into test code
        let take_profit = BackwardsCompatTakeProfit {
            leverage,
            direction,
            collateral,
            market_type: self.id.get_market_type(),
            max_gains,
            take_profit: take_profit_override,
            price_point: &self.query_current_price()?,
        }
        .calc()?;

        let msg = self.token.into_market_execute_msg(
            &self.addr,
            collateral.raw(),
            ExecuteMsg::PlaceLimitOrder {
                trigger_price,
                leverage,
                direction,
                max_gains: None,
                stop_loss_override,
                take_profit: Some(take_profit),
            },
        )?;

        let defer_res = self.exec_defer_wasm_msg(sender, msg)?;

        let order_id = defer_res
            .exec_resp()
            .event_first_value(event_key::PLACE_LIMIT_ORDER, event_key::ORDER_ID)?
            .parse()?;

        Ok((order_id, defer_res))
    }

    pub fn exec_cancel_limit_order(
        &self,
        sender: &Addr,
        order_id: OrderId,
    ) -> Result<DeferResponse> {
        self.exec_defer(sender, &MarketExecuteMsg::CancelLimitOrder { order_id })
    }

    // outside contract queries that require market info like addr
    pub fn query_liquidity_token_addr(&self, kind: LiquidityTokenKind) -> Result<Addr> {
        let market_id = self.id.clone();
        let res: MarketInfoResponse =
            self.query_factory(&FactoryQueryMsg::MarketInfo { market_id })?;

        Ok(match kind {
            LiquidityTokenKind::Lp => res.liquidity_token_lp,
            LiquidityTokenKind::Xlp => res.liquidity_token_xlp,
        })
    }

    pub(crate) fn query_position_token_addr(&self) -> Result<Addr> {
        let market_id = self.id.clone();
        let res: MarketInfoResponse =
            self.query_factory(&FactoryQueryMsg::MarketInfo { market_id })?;
        Ok(res.position_token)
    }

    pub fn query_factory_copy_contracts(&self) -> Result<CopyTradingResp> {
        let res: CopyTradingResp = self.query_factory(&FactoryQueryMsg::CopyTrading {
            start_after: None,
            limit: None,
        })?;
        Ok(res)
    }

    pub fn query_factory_copy_contracts_leader(&self, leader: &Addr) -> Result<CopyTradingResp> {
        let res: CopyTradingResp = self.query_factory(&FactoryQueryMsg::CopyTradingForLeader {
            leader: leader.into(),
            start_after: None,
            limit: None,
        })?;
        Ok(res)
    }

    pub fn query_position_token_owner(&self, token_id: &str) -> Result<Addr> {
        let contract_addr = self.query_position_token_addr()?;
        let resp: OwnerOfResponse = self.app().cw721_query(
            contract_addr,
            &Cw721QueryMsg::OwnerOf {
                token_id: token_id.to_string(),
                include_expired: None,
            },
        )?;

        Ok(Addr::unchecked(resp.owner))
    }
    pub fn query_position_token_metadata(&self, token_id: &str) -> Result<Cw721Metadata> {
        let contract_addr = self.query_position_token_addr()?;
        let resp: NftInfoResponse = self.app().cw721_query(
            contract_addr,
            &Cw721QueryMsg::NftInfo {
                token_id: token_id.to_string(),
            },
        )?;

        Ok(resp.extension)
    }

    pub fn query_position_token_ids(&self, owner: &Addr) -> Result<Vec<String>> {
        let contract_addr = self.query_position_token_addr()?;

        let resp: TokensResponse = self.app().cw721_query(
            contract_addr,
            &Cw721QueryMsg::Tokens {
                owner: owner.clone().into(),
                start_after: None,
                limit: None,
            },
        )?;

        Ok(resp.tokens)
    }

    /// Get the referrer for a referee
    pub fn query_referrer(&self, referee: &Addr) -> Result<Option<Addr>> {
        Ok(
            match self.query_factory(&FactoryQueryMsg::GetReferrer {
                addr: referee.as_str().into(),
            })? {
                GetReferrerResp::NoReferrer {} => None,
                GetReferrerResp::HasReferrer { referrer } => Some(referrer),
            },
        )
    }

    /// Register a referrer for the given referee.
    ///
    /// Referee comes first in argument order.
    pub fn exec_register_referrer(&self, referee: &Addr, referrer: &Addr) -> Result<()> {
        self.exec_factory_as(
            referee,
            &FactoryExecuteMsg::RegisterReferrer {
                addr: referrer.into(),
            },
        )
        .map(|_| ())
    }

    /// Get all the referees for a referrer
    pub fn query_referees(&self, referrer: &Addr) -> Result<Vec<Addr>> {
        let mut ret = vec![];
        let mut start_after = None;
        let total = self.query_referral_stats(referrer)?.referees;
        loop {
            let ListRefereesResp {
                mut referees,
                next_start_after,
            } = self.query_factory(&FactoryQueryMsg::ListReferees {
                addr: referrer.into(),
                limit: None,
                start_after: start_after.take(),
            })?;
            ret.append(&mut referees);
            match next_start_after {
                None => {
                    anyhow::ensure!(ret.len() == usize::try_from(total)?);
                    break Ok(ret);
                }
                Some(next_start_after) => start_after = Some(next_start_after),
            }
        }
    }

    pub fn query_referrer_counts(&self) -> Result<HashMap<Addr, u32>> {
        let mut ret = HashMap::new();
        let mut start_after = None;
        loop {
            let ListRefereeCountResp {
                counts,
                next_start_after,
            } = self.query_factory(&FactoryQueryMsg::ListRefereeCount {
                limit: None,
                start_after: start_after.take(),
            })?;
            for RefereeCount { referrer, count } in counts {
                anyhow::ensure!(!ret.contains_key(&referrer));
                ret.insert(referrer, count);
            }
            match next_start_after {
                None => {
                    break Ok(ret);
                }
                Some(next_start_after) => start_after = Some(next_start_after),
            }
        }
    }

    /// Perform a shutdown action
    pub fn exec_shutdown(
        &self,
        sender: &Addr,
        effect: ShutdownEffect,
        markets: &[&MarketId],
        impacts: &[ShutdownImpact],
    ) -> Result<AppResponse> {
        self.exec_factory_as(
            sender,
            &FactoryExecuteMsg::Shutdown {
                markets: markets.iter().copied().cloned().collect(),
                impacts: impacts.to_owned(),
                effect,
            },
        )
    }

    /// Close all open positions
    pub fn exec_close_all_positions(&self, sender: &Addr) -> Result<AppResponse> {
        self.exec(sender, &MarketExecuteMsg::CloseAllPositions {})
    }

    pub fn exec_position_token_transfer(
        &self,
        token_id: &str,
        from: &Addr,
        to: &Addr,
    ) -> Result<AppResponse> {
        let contract_addr = self.query_position_token_addr()?;
        self.app().cw721_exec(
            from.clone(),
            contract_addr,
            &Cw721ExecuteMsg::TransferNft {
                recipient: to.clone().into(),
                token_id: token_id.to_string(),
            },
        )
    }

    pub fn exec_liquidity_token_send(
        &self,
        kind: LiquidityTokenKind,
        from: &Addr,
        contract: &Addr,
        amount: LpToken,
        msg: &impl Serialize,
    ) -> Result<AppResponse> {
        let token_info = self.query_liquidity_token_info(kind)?;
        self.exec_liquidity_token_send_raw(
            kind,
            from,
            contract,
            amount
                .into_number()
                .to_u128_with_precision(token_info.decimals as u32)
                .context("couldnt convert liquidity token amount")?
                .into(),
            to_json_binary(msg)?,
        )
    }

    pub fn exec_liquidity_token_send_from(
        &self,
        kind: LiquidityTokenKind,
        wallet: &Addr,
        owner: &Addr,
        contract: &Addr,
        amount: LpToken,
        msg: &impl Serialize,
    ) -> Result<AppResponse> {
        let token_info = self.query_liquidity_token_info(kind)?;
        self.exec_liquidity_token_send_from_raw(
            kind,
            wallet,
            owner,
            contract,
            amount
                .into_number()
                .to_u128_with_precision(token_info.decimals as u32)
                .context("couldnt convert liquidity token amount")?
                .into(),
            to_json_binary(msg)?,
        )
    }
    fn exec_liquidity_token_send_raw(
        &self,
        kind: LiquidityTokenKind,
        from: &Addr,
        contract: &Addr,
        amount: Uint128,
        msg: Binary,
    ) -> Result<AppResponse> {
        let contract_addr = self.query_liquidity_token_addr(kind)?;

        self.app().cw20_exec(
            from,
            &contract_addr,
            &Cw20ExecuteMsg::Send {
                contract: contract.clone().into(),
                amount,
                msg,
            },
        )
    }

    fn exec_liquidity_token_send_from_raw(
        &self,
        kind: LiquidityTokenKind,
        wallet: &Addr,
        owner: &Addr,
        contract: &Addr,
        amount: Uint128,
        msg: Binary,
    ) -> Result<AppResponse> {
        let contract_addr = self.query_liquidity_token_addr(kind)?;

        self.app().cw20_exec(
            wallet,
            &contract_addr,
            &Cw20ExecuteMsg::SendFrom {
                owner: owner.into(),
                contract: contract.clone().into(),
                amount,
                msg,
            },
        )
    }

    pub fn exec_liquidity_token_transfer(
        &self,
        kind: LiquidityTokenKind,
        from: &Addr,
        recipient: &Addr,
        amount: Number,
    ) -> Result<AppResponse> {
        let token_info = self.query_liquidity_token_info(kind)?;
        self.exec_liquidity_token_transfer_raw(
            kind,
            from,
            recipient,
            amount
                .to_u128_with_precision(token_info.decimals as u32)
                .context("couldnt convert liquidity token amount")?
                .into(),
        )
    }

    pub(crate) fn exec_liquidity_token_transfer_raw(
        &self,
        kind: LiquidityTokenKind,
        from: &Addr,
        recipient: &Addr,
        amount: Uint128,
    ) -> Result<AppResponse> {
        let contract_addr = self.query_liquidity_token_addr(kind)?;

        self.app().cw20_exec(
            from,
            &contract_addr,
            &Cw20ExecuteMsg::Transfer {
                recipient: recipient.into(),
                amount,
            },
        )
    }
    pub fn exec_liquidity_token_increase_allowance(
        &self,
        kind: LiquidityTokenKind,
        wallet: &Addr,
        spender: &Addr,
        amount: Number,
    ) -> Result<AppResponse> {
        let contract_addr = self.query_liquidity_token_addr(kind)?;

        let token_info = self.query_liquidity_token_info(kind)?;

        let amount = amount
            .to_u128_with_precision(token_info.decimals as u32)
            .context("couldnt convert liquidity token amount")?
            .into();

        self.app().cw20_exec(
            wallet,
            &contract_addr,
            &Cw20ExecuteMsg::IncreaseAllowance {
                spender: spender.into(),
                amount,
                expires: None,
            },
        )
    }

    pub fn exec_factory(&self, msg: &FactoryExecuteMsg) -> Result<AppResponse> {
        self.exec_factory_as(&Addr::unchecked(&TEST_CONFIG.protocol_owner), msg)
    }

    pub fn exec_factory_as(&self, sender: &Addr, msg: &FactoryExecuteMsg) -> Result<AppResponse> {
        let contract_addr = self.app().factory_addr.clone();
        let res = self
            .app()
            .execute_contract(sender.clone(), contract_addr, msg, &[])?;

        self.set_time(TimeJump::Blocks(1))?;

        Ok(res)
    }

    pub fn query_factory_raw(&self, key: impl Into<Binary>) -> Result<Option<Vec<u8>>> {
        let contract_addr = self.app().factory_addr.clone();
        let result = self.app().wrap().query_wasm_raw(contract_addr, key)?;
        Ok(result)
    }

    // deliberately use the cw20 msg, not liquidity_token
    pub fn query_liquidity_token<T: DeserializeOwned>(
        &self,
        kind: LiquidityTokenKind,
        msg: &Cw20QueryMsg,
    ) -> Result<T> {
        let contract_addr = self.query_liquidity_token_addr(kind)?;
        self.app()
            .wrap()
            .query_wasm_smart(contract_addr, &msg)
            .map_err(|err| err.into())
    }

    pub(crate) fn query_liquidity_token_info(
        &self,
        kind: LiquidityTokenKind,
    ) -> Result<TokenInfoResponse> {
        self.query_liquidity_token(kind, &Cw20QueryMsg::TokenInfo {})
    }

    pub fn query_liquidity_token_balance_raw(
        &self,
        kind: LiquidityTokenKind,
        user: &Addr,
    ) -> Result<Uint128> {
        let resp: BalanceResponse = self.query_liquidity_token(
            kind,
            &Cw20QueryMsg::Balance {
                address: user.into(),
            },
        )?;

        Ok(resp.balance)
    }

    pub fn query_factory<T: DeserializeOwned>(&self, msg: &FactoryQueryMsg) -> Result<T> {
        let contract_addr = self.app().factory_addr.clone();
        self.app()
            .wrap()
            .query_wasm_smart(contract_addr, &msg)
            .map_err(|err| err.into())
    }

    pub fn query_shutdown_status(&self, market_id: &MarketId) -> Result<Vec<ShutdownImpact>> {
        let ShutdownStatus { disabled } = self.query_factory(&FactoryQueryMsg::ShutdownStatus {
            market_id: market_id.clone(),
        })?;
        Ok(disabled)
    }

    pub fn handle_bridge_msg<A, B, C>(
        &self,
        wrapper: &ClientToBridgeWrapper,
        on_exec: A,
        on_query: B,
        on_time_jump: C,
    ) where
        A: Fn(Result<AppResponse>),
        B: Fn(Result<Binary>),
        C: Fn(Result<i64>),
    {
        match &wrapper.msg {
            ClientToBridgeMsg::MintCollateral { amount } => {
                on_exec(self.exec_mint_tokens(&wrapper.user, amount.into_number()));
            }
            ClientToBridgeMsg::MintAndDepositLp { amount } => {
                on_exec(self.exec_mint_and_deposit_liquidity(&wrapper.user, amount.into_number()));
            }

            ClientToBridgeMsg::RefreshPrice => {
                on_exec(self.exec_refresh_price().map(|res| res.base));
            }

            ClientToBridgeMsg::Crank => {
                on_exec(self.exec_crank(&wrapper.user));
            }

            ClientToBridgeMsg::ExecMarket { exec_msg, funds } => {
                on_exec(match funds {
                    Some(funds) => self.exec_funds(&wrapper.user, exec_msg, funds.into_number()),
                    None => self.exec(&wrapper.user, exec_msg),
                });
            }
            ClientToBridgeMsg::QueryMarket { query_msg } => {
                on_query(self.raw_query(query_msg));
            }
            ClientToBridgeMsg::TimeJumpSeconds { seconds } => {
                on_time_jump(self.set_time(TimeJump::Seconds(*seconds)).map(|_| *seconds));
            }
        }
    }

    // this defers a message exec in the sense of Levana perps semantics of "deferred executions"
    // *not* defer in the sense of native programming jargon, like the golang keyword or until Drop kicks in etc.
    // this does *not* automatic time jump
    pub fn exec_defer_queue_wasm_msg(
        &self,
        sender: &Addr,
        msg: WasmMsg,
    ) -> Result<DeferQueueResponse> {
        let cosmos_msg = CosmosMsg::Wasm(msg);
        let res = self.app().execute(sender.clone(), cosmos_msg)?;

        let queue_event = res
            .event_first("deferred-exec-queued")
            .and_then(DeferredExecQueuedEvent::try_from)?;

        let value = self.query_deferred_exec(queue_event.deferred_exec_id)?;

        match value.status {
            DeferredExecStatus::Failure { reason, .. } => Err(anyhow!("{}", reason)),
            _ => Ok(DeferQueueResponse {
                event: queue_event,
                value,
                response: res,
            }),
        }
    }

    pub fn query_deferred_exec(&self, id: DeferredExecId) -> Result<DeferredExecWithStatus> {
        let item: GetDeferredExecResp = self.query(&QueryMsg::GetDeferredExec { id })?;
        match item {
            GetDeferredExecResp::Found { item } => Ok(*item),
            GetDeferredExecResp::NotFound {} => {
                Err(anyhow!("deferred item with id {} not found", id))
            }
        }
    }

    // this *does* automatic time jump
    // at least in the sense of cranking/moving forward until it gets the deferred execution
    // i.e. if it happens to be that the queue response is _also_ the exec response, then it won't move forward - otherwise it will
    pub fn exec_defer_queue_process(
        &self,
        cranker: &Addr,
        queue: DeferQueueResponse,
        crank_execs: Option<u32>,
    ) -> Result<DeferResponse> {
        let mut responses = vec![queue.response];

        // this loops forever if the deferred execution never *happens*
        // which would be a core bug since by this point it's definitely been queued
        // it will stop looping whether the deferred execution succeeds or fails
        // and success/failure can be determined by looking at DeferResponse::exec_event.success
        loop {
            if self.debug_001 {
                //println!("{:#?}", responses.last().as_ref().unwrap());
                println!("deferred execution items: {}, next crank: {:#?}, next deferred execution: {:#?}", self.query_status().unwrap().deferred_execution_items, self.query_status().unwrap().next_crank, self.query_status().unwrap().next_deferred_execution);
            }
            // check before cranking, in case the deferred execution was queued and completed in the same block
            if let Ok(exec_event) = responses
                .last()
                .as_ref()
                .unwrap()
                .event_first("deferred-exec-executed")
                .and_then(DeferredExecExecutedEvent::try_from)
            {
                if exec_event.deferred_exec_id == queue.event.deferred_exec_id {
                    let value = self.query_deferred_exec(exec_event.deferred_exec_id)?;
                    break match exec_event.success {
                        true => match value.status {
                            DeferredExecStatus::Success { .. } => Ok(DeferResponse {
                                exec_event,
                                queue_event: queue.event,
                                value,
                                responses,
                            }),
                            _ => Err(anyhow!(
                                "expected deferred status of success, but it's {:?}",
                                value.status
                            )),
                        },
                        false => match &value.status {
                            DeferredExecStatus::Failure {
                                reason,
                                crank_price,
                                ..
                            } => match &crank_price {
                                None => {
                                    panic!(
                                            "crank price is none in deferred exec - this is a core unexpected error: {:?}",
                                            value.status
                                        );
                                }
                                Some(_) if reason.contains("error executing WasmMsg") => {
                                    panic!(
                                            "validation is passing but it should be failing- this is a core unexpected error: {:?}",
                                            value.status
                                        );
                                }
                                _ => Err(anyhow!("{}", reason)),
                            },
                            _ => Err(anyhow!(
                                "expected deferred status of failure, but it's {:?}",
                                value.status
                            )),
                        },
                    };
                }
            }

            // this condition could removed eventually, but for now it might be helpful
            // to catch bugs in migrating the test code that rely on the time not moving
            if !self.automatic_time_jump_enabled {
                bail!("automatic time jump is not enabled, cannot defer wasm msg")
            }

            // This doesn't seem necessary so far...
            // if it becomes necessary, maybe check to make sure we really need a price update here
            // self.exec_refresh_price()?;
            responses.push(self.exec(
                cranker,
                &MarketExecuteMsg::Crank {
                    execs: crank_execs,
                    rewards: None,
                },
            )?);
        }
    }

    pub fn exec_defer_wasm_msg(&self, sender: &Addr, msg: WasmMsg) -> Result<DeferResponse> {
        let queue_res = self.exec_defer_queue_wasm_msg(sender, msg)?;
        self.exec_defer_queue_process(sender, queue_res, None)
    }

    pub fn get_countertrade_addr(&self) -> Addr {
        self.app().countertrade_addr.clone()
    }

    pub(crate) fn query_countertrade<T: DeserializeOwned>(
        &self,
        msg: &CountertradeQueryMsg,
    ) -> Result<T> {
        let contract_addr = self.app().countertrade_addr.clone();
        self.app()
            .wrap()
            .query_wasm_smart(contract_addr, &msg)
            .map_err(|err| err.into())
    }

    pub(crate) fn query_copy_trading<T: DeserializeOwned>(
        &self,
        msg: &CopyTradingQueryMsg,
    ) -> Result<T> {
        let contract_addr = self.copy_trading_addr.clone();
        self.app()
            .wrap()
            .query_wasm_smart(contract_addr, &msg)
            .map_err(|err| err.into())
    }

    pub fn query_copy_trading_queue_status(
        &self,
        wallet: RawAddr,
        start_after: Option<msg::contracts::copy_trading::QueuePositionId>,
        limit: Option<u32>,
    ) -> Result<msg::contracts::copy_trading::QueueResp> {
        self.query_copy_trading(&CopyTradingQueryMsg::QueueStatus {
            address: wallet,
            start_after,
            limit,
        })
    }

    pub fn query_copy_trading_work(&self) -> Result<msg::contracts::copy_trading::WorkResp> {
        self.query_copy_trading(&CopyTradingQueryMsg::HasWork {})
    }

    pub fn query_copy_trading_leader_tokens(
        &self,
    ) -> Result<msg::contracts::copy_trading::LeaderStatusResp> {
        self.query_copy_trading(&CopyTradingQueryMsg::LeaderStatus {
            start_after: None,
            limit: None,
        })
    }

    pub fn query_copy_trading_balance(
        &self,
        wallet: &Addr,
    ) -> Result<msg::contracts::copy_trading::BalanceResp> {
        self.query_copy_trading(&CopyTradingQueryMsg::Balance {
            address: wallet.into(),
            start_after: None,
            limit: None,
        })
    }

    pub fn query_copy_trading_config(&self) -> Result<CopyTradingConfig> {
        self.query_copy_trading(&CopyTradingQueryMsg::Config {})
    }

    pub fn query_countertrade_config(&self) -> Result<CountertradeConfig> {
        self.query_countertrade(&CountertradeQueryMsg::Config {})
    }

    pub fn query_countertrade_has_work(&self) -> Result<HasWorkResp> {
        self.query_countertrade(&CountertradeQueryMsg::HasWork {
            market: self.id.clone(),
        })
    }

    pub fn query_countertrade_balances(
        &self,
        user_addr: &Addr,
    ) -> Result<Vec<msg::contracts::countertrade::MarketBalance>> {
        let mut start_after = None;
        let mut res = vec![];
        loop {
            let msg::contracts::countertrade::BalanceResp {
                mut markets,
                next_start_after,
            } = self.query_countertrade(&CountertradeQueryMsg::Balance {
                address: user_addr.into(),
                start_after: start_after.take(),
                limit: None,
            })?;
            res.append(&mut markets);
            match next_start_after {
                Some(next_start_after) => start_after = Some(next_start_after),
                None => break Ok(res),
            }
        }
    }

    pub fn query_countertrade_markets(
        &self,
    ) -> Result<Vec<msg::contracts::countertrade::MarketStatus>> {
        let mut start_after = None;
        let mut res = vec![];
        loop {
            let msg::contracts::countertrade::MarketsResp {
                mut markets,
                next_start_after,
            } = self.query_countertrade(&CountertradeQueryMsg::Markets {
                start_after: start_after.take(),
                limit: None,
            })?;
            res.append(&mut markets);
            match next_start_after {
                Some(next_start_after) => start_after = Some(next_start_after),
                None => break Ok(res),
            }
        }
    }

    pub fn query_countertrade_market_id(
        &self,
        market_id: MarketId,
    ) -> Result<msg::contracts::countertrade::MarketStatus> {
        let result = self.query_countertrade_markets()?;
        let res = result.into_iter().find(|item| item.id == market_id);
        res.context("Market id {market_id} not found")
    }

    pub fn get_copytrading_token(&self) -> Result<msg::contracts::copy_trading::Token> {
        let token = self.token.clone();
        match token {
            Token::Cw20 { addr, .. } => {
                let cw20 = addr.validate(self.app().api())?;
                Ok(msg::contracts::copy_trading::Token::Cw20(cw20))
            }
            Token::Native { denom, .. } => Ok(msg::contracts::copy_trading::Token::Native(denom)),
        }
    }

    pub fn exec_ct_leader(&self, amount: &str) -> Result<AppResponse> {
        let amount = amount.parse()?;
        let market_id = self.id.clone();
        let leverage = "7".parse().unwrap();
        let msg = Box::new(MarketExecuteMsg::OpenPosition {
            slippage_assert: None,
            leverage,
            direction: DirectionToBase::Long,
            max_gains: None,
            stop_loss_override: None,
            take_profit: Some(TakeProfitTrader::Finite("1.1".parse().unwrap())),
        });
        let wasm_msg = &CopyTradingExecuteMsg::LeaderMsg {
            market_id,
            message: msg,
            collateral: Some(amount),
        };
        let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());
        self.exec_copytrading(&leader, wasm_msg)
    }

    pub fn exec_copytrading_mint_and_deposit(
        &self,
        sender: &Addr,
        amount: &str,
    ) -> Result<AppResponse> {
        let amount: Collateral = amount.parse()?;
        self.exec_mint_tokens(sender, amount.into_number())?;
        let wasm_msg = self.make_msg_with_funds(
            &CopyTradingExecuteMsg::Deposit {},
            amount.into_number(),
            &self.copy_trading_addr,
        )?;
        self.exec_wasm_msg(sender, wasm_msg)
    }

    pub fn exec_countertrade_mint_and_deposit(
        &self,
        user_addr: &Addr,
        amount: &str,
    ) -> Result<AppResponse> {
        let amount: Collateral = amount.parse()?;
        self.exec_mint_tokens(user_addr, amount.into_number())?;
        let wasm_msg = self.make_msg_with_funds(
            &CountertradeExecuteMsg::Deposit {
                market: self.id.clone(),
            },
            amount.into_number(),
            &self.app().countertrade_addr,
        )?;
        self.exec_wasm_msg(user_addr, wasm_msg)
    }

    fn exec_countertrade(
        &self,
        sender: &Addr,
        msg: &CountertradeExecuteMsg,
    ) -> Result<AppResponse> {
        let contract_addr = self.app().countertrade_addr.clone();
        let res = self
            .app()
            .execute_contract(sender.clone(), contract_addr, msg, &[])?;

        Ok(res)
    }

    pub fn exec_copytrading(
        &self,
        sender: &Addr,
        msg: &CopyTradingExecuteMsg,
    ) -> Result<AppResponse> {
        let contract_addr = self.copy_trading_addr.clone();
        let res = self
            .app()
            .execute_contract(sender.clone(), contract_addr, msg, &[])?;

        Ok(res)
    }

    pub fn exec_copytrading_do_work(&self, sender: &Addr) -> Result<AppResponse> {
        self.exec_copytrading(sender, &CopyTradingExecuteMsg::DoWork {})
    }

    pub fn exec_copytrading_withdrawal(&self, sender: &Addr, amount: &str) -> Result<AppResponse> {
        let amount = amount.parse()?;
        let amount = NonZero::new(amount).context("amount is zero")?;
        let token = self.get_copytrading_token()?;
        self.exec_copytrading(
            sender,
            &CopyTradingExecuteMsg::Withdraw {
                shares: amount,
                token,
            },
        )
    }

    pub fn exec_countertrade_withdraw(&self, sender: &Addr, amount: &str) -> Result<AppResponse> {
        self.exec_countertrade(
            sender,
            &CountertradeExecuteMsg::Withdraw {
                amount: amount.parse()?,
                market: self.id.clone(),
            },
        )
    }

    pub fn exec_countertrade_do_work(&self) -> Result<AppResponse> {
        let owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
        self.exec_countertrade(
            // Could be anyone
            &owner,
            &CountertradeExecuteMsg::DoWork {
                market: self.id.clone(),
            },
        )
    }

    pub fn exec_countertrade_appoint_admin(&self, new_admin: &Addr) -> Result<AppResponse> {
        let owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
        self.exec_countertrade(
            &owner,
            &CountertradeExecuteMsg::AppointAdmin {
                admin: new_admin.into(),
            },
        )
    }

    pub fn exec_countertrade_accept_admin(&self, new_admin: &Addr) -> Result<AppResponse> {
        self.exec_countertrade(new_admin, &CountertradeExecuteMsg::AcceptAdmin {})
    }

    pub fn exec_countertrade_update_config(
        &self,
        update: msg::contracts::countertrade::ConfigUpdate,
    ) -> Result<AppResponse> {
        let owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
        self.exec_countertrade(&owner, &CountertradeExecuteMsg::UpdateConfig(update))
    }
}

#[derive(Debug)]
pub struct DeferResponse {
    pub queue_event: DeferredExecQueuedEvent,
    pub exec_event: DeferredExecExecutedEvent,
    pub value: DeferredExecWithStatus,
    pub responses: Vec<AppResponse>,
}

impl DeferResponse {
    pub fn queue_resp(&self) -> &AppResponse {
        // Safe - we only get here if the deferred execution was queued, and by definition that's the first AppResponse we get
        self.responses.first().unwrap()
    }

    pub fn exec_resp(&self) -> &AppResponse {
        // Safe - we only get here if the deferred execution was executed, and by definition that's the last AppResponse we get
        self.responses.last().unwrap()
    }
}

#[derive(Clone, Debug)]
pub struct DeferQueueResponse {
    pub event: DeferredExecQueuedEvent,
    pub value: DeferredExecWithStatus,
    pub response: AppResponse,
}

#[derive(Clone, Debug)]
pub struct PriceResponse {
    pub base: AppResponse,
    // will be identical to the response for `base` for manual prices
    pub usd: AppResponse,
}
