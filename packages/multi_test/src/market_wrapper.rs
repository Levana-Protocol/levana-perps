/*
    High-level concepts:

    1. All executions that go through the market jump to the next block after
    2. All time jumps move the block height as well

    The basic idea is that it simulates real-world usage
    tests which require manipulating the underlying machinery at a lower level
    must do so via the app, not market
*/

use super::PerpsApp;
use crate::config::{TokenKind, DEFAULT_MARKET, TEST_CONFIG};
use crate::response::CosmosResponseExt;
use crate::time::{BlockInfoChange, TimeJump};
use anyhow::Context;
pub use anyhow::{anyhow, Result};
use cosmwasm_std::{
    to_binary, to_vec, Addr, Binary, Coin, ContractResult, CosmosMsg, Empty, QueryRequest,
    StdError, SystemResult, Uint128, WasmMsg, WasmQuery,
};
use cw_multi_test::{AppResponse, BankSudo, Executor, SudoMsg};
use msg::bridge::{ClientToBridgeMsg, ClientToBridgeWrapper};
use msg::contracts::cw20::entry::{
    BalanceResponse, ExecuteMsg as Cw20ExecuteMsg, QueryMsg as Cw20QueryMsg, TokenInfoResponse,
};
use msg::contracts::factory::entry::{
    ExecuteMsg as FactoryExecuteMsg, MarketInfoResponse, QueryMsg as FactoryQueryMsg,
    ShutdownStatus,
};
use msg::contracts::farming::entry::{
    ExecuteMsg as FarmingExecuteMsg, FarmersResp, LockdropBucketId, OwnerExecuteMsg,
    QueryMsg as FarmingQueryMsg,
};
use msg::contracts::farming::entry::{
    FarmerStats, OwnerExecuteMsg as FarmingOwnerExecuteMsg, StatusResp as FarmingStatusResp,
};
use msg::contracts::liquidity_token::LiquidityTokenKind;
use msg::contracts::market::crank::CrankWorkInfo;
use msg::contracts::market::entry::{
    ClosedPositionCursor, ClosedPositionsResp, DeltaNeutralityFeeResp, ExecuteMsg, Fees,
    LimitOrderHistoryResp, LimitOrderResp, LimitOrdersResp, LpAction, LpActionHistoryResp,
    LpInfoResp, PositionActionHistoryResp, PositionsQueryFeeApproach, PriceForQuery,
    PriceWouldTriggerResp, QueryMsg, SlippageAssert, SpotPriceHistoryResp, StatusResp,
    TradeHistorySummary, TraderActionHistoryResp,
};
use msg::contracts::market::position::{ClosedPosition, PositionsResp};
use msg::contracts::market::spot_price::{SpotPriceConfig, SpotPriceFeed, SpotPriceFeedData};
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

use msg::constants::event_key;
use msg::contracts::farming::entry::OwnerExecuteMsg::ReclaimEmissions;
use msg::contracts::market::order::OrderId;
use msg::shared::cosmwasm::OrderInMessage;
use msg::shutdown::{ShutdownEffect, ShutdownImpact};
use msg::token::{Token, TokenInit};
use rand::rngs::ThreadRng;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::cell::{RefCell, RefMut};
use std::rc::Rc;

pub struct PerpsMarket {
    // we can have multiple markets per app instance
    // PerpsApp is not thread-safe, however (i.e. it's RefCell not Mutex here)
    app: Rc<RefCell<PerpsApp>>,
    pub token: Token,
    pub id: MarketId,
    pub addr: Addr,
    /// When enabled, time will jump by one block on every exec
    pub automatic_time_jump_enabled: bool,
    /// Address of the farming contract
    farming_addr: Addr,
}

impl PerpsMarket {
    pub fn new(app: Rc<RefCell<PerpsApp>>) -> Result<Self> {
        Self::new_with_type(app, DEFAULT_MARKET.collateral_type, true)
    }

    pub fn new_with_type(
        app: Rc<RefCell<PerpsApp>>,
        market_type: MarketType,
        bootstap_lp: bool,
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
            bootstap_lp,
        )
    }

    pub fn new_custom(
        app: Rc<RefCell<PerpsApp>>,
        id: MarketId,
        token_init: TokenInit,
        initial_price: PriceBaseInQuote,
        collateral_in_usd: Option<PriceCollateralInUsd>,
        bootstap_lp: bool,
    ) -> Result<Self> {
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
                    // Easier to just go back to the original default than update tests
                    unstake_period_seconds: Some(60 * 60 * 24 * 21),
                    // Same: original default to fix tests
                    trading_fee_notional_size: Some("0.0005".parse().unwrap()),
                    trading_fee_counter_collateral: Some("0.0005".parse().unwrap()),
                    liquidity_cooldown_seconds: Some(0),
                    ..Default::default()
                }),
                spot_price: SpotPriceConfig {
                    feeds: vec![SpotPriceFeed { 
                        data: SpotPriceFeedData::Manual {
                            id: DEFAULT_MARKET.spot_price_id.clone(),
                        },
                        inverted: false 
                    }],
                    feeds_usd: None, 
                },
                initial_borrow_fee_rate: "0.01".parse().unwrap(),
            },
        };

        let factory_addr = app.borrow().factory_addr.clone();

        let protocol_owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
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
            .find(|a| a.key == "_contract_addr")
            .context("could not instantiate")?
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

        let farming_code_id = app.borrow().code_id(crate::PerpsContract::Farming)?;
        let farming_addr = app.borrow_mut().instantiate_contract(
            farming_code_id,
            protocol_owner.clone(),
            &msg::contracts::farming::entry::InstantiateMsg {
                owner: protocol_owner.clone().into(),
                factory: factory_addr.into(),
                market_id: id.clone(),
                lockdrop_month_seconds:
                    msg::contracts::farming::entry::defaults::lockdrop_month_seconds(),
                lockdrop_buckets: msg::contracts::farming::entry::defaults::lockdrop_buckets(),
                bonus_ratio: msg::contracts::farming::entry::defaults::bonus_ratio(),
                bonus_addr: protocol_owner.clone().into(),
                lockdrop_lvn_unlock_seconds:
                    msg::contracts::farming::entry::defaults::lockdrop_month_seconds(),
                lockdrop_immediate_unlock_ratio:
                    msg::contracts::farming::entry::defaults::lockdrop_immediate_unlock_ratio(),
                lvn_token_denom: TEST_CONFIG.rewards_token_denom.clone(),
                lockdrop_start_duration: 60 * 60 * 24 * 12,
                lockdrop_sunset_duration: 60 * 60 * 24 * 2,
            },
            &[],
            "Farming Contract".to_owned(),
            Some(protocol_owner.into_string()),
        )?;

        let mut _self = Self {
            app,
            id,
            token,
            addr: market_addr,
            automatic_time_jump_enabled: true,
            farming_addr,
        };

        _self.exec_set_price_with_usd(initial_price, collateral_in_usd)?;

        if bootstap_lp {
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
            config.staleness_seconds as u64,
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
            msg: to_binary(msg)?,
        }
        .into();

        let raw = to_vec(&request).map_err(|serialize_err| {
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

    pub fn make_msg_with_funds(&self, msg: &MarketExecuteMsg, amount: Number) -> Result<WasmMsg> {
        let amount = Collateral::from_decimal256(
            amount
                .try_into_positive_value()
                .context("funds must be positive!")?,
        );

        let market_addr = self.addr.clone();

        Ok(match NonZero::new(amount) {
            None => WasmMsg::Execute {
                contract_addr: market_addr.to_string(),
                msg: to_binary(msg)?,
                funds: vec![],
            },
            Some(amount) => {
                self.token
                    .into_market_execute_msg(&market_addr, amount.raw(), msg.clone())?
            }
        })
    }

    pub fn exec_funds(
        &self,
        sender: &Addr,
        msg: &MarketExecuteMsg,
        amount: Number,
    ) -> Result<AppResponse> {
        let wasm_msg = self.make_msg_with_funds(msg, amount)?;
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
                    == unstaking.collected + unstaking.available + unstaking.pending,
                "Incoherent unstaking value: {unstaking:?}"
            );
        }
        Ok(lp_info_resp)
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
    pub fn exec_refresh_price(&self) -> Result<AppResponse> {
        // we used to have an explicit "set price" message. 
        // Now it happens automatically through other actions
        // TODO: remove this and just explicitly crank where needed?
        self.exec_crank_n(&Addr::unchecked("refresh-price"), 1)
    }
    pub fn exec_set_price(&self, price: PriceBaseInQuote) -> Result<Vec<AppResponse>> {
        self.exec_set_price_with_usd(price, None)
    }

    pub fn exec_set_price_and_crank(&self, price: PriceBaseInQuote) -> Result<Vec<AppResponse>> {
        let mut responses = self.exec_set_price_with_usd(price, None)?;
        responses.push(self.exec_crank_n(&Addr::unchecked("set-price"), 1)?);

        Ok(responses)
    }

    pub fn exec_set_price_with_usd(
        &self,
        price: PriceBaseInQuote,
        price_usd: Option<PriceCollateralInUsd>,
    ) -> Result<Vec<AppResponse>> {
        let mut responses = Vec::new();

        responses.push(self.exec(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &MarketExecuteMsg::Owner(MarketExecuteOwnerMsg::SetManualPrice { 
                id: DEFAULT_MARKET.spot_price_id.clone(), 
                price: price.into_non_zero()
            })
        )?);

        if let Some(price_usd) = price_usd {
            responses.push(self.exec(
                &Addr::unchecked(&TEST_CONFIG.protocol_owner),
                &MarketExecuteMsg::Owner(MarketExecuteOwnerMsg::SetManualPrice { 
                    id: DEFAULT_MARKET.spot_price_usd_id.clone(), 
                    price: price_usd.into_number().try_into_non_zero().context("price must be greater than zero")?
                })
            )?);
        }

        Ok(responses)
    }

    pub fn exec_set_config(&self, config_update: ConfigUpdate) -> Result<AppResponse> {
        self.exec(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &MarketExecuteMsg::Owner(MarketExecuteOwnerMsg::ConfigUpdate {
                update: config_update,
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

        while self.query_crank_stats()?.is_some() {
            let resp = self.exec(
                sender,
                &MarketExecuteMsg::Crank {
                    execs: None,
                    rewards: None,
                },
            )?;

            responses.push(resp);
        }

        Ok(responses)
    }

    pub fn exec_crank_till_finished_with_rewards(
        &self,
        sender: &Addr,
        rewards: &Addr,
    ) -> Result<Vec<AppResponse>> {
        let mut responses = Vec::new();

        while self.query_crank_stats()?.is_some() {
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
    ) -> Result<(PositionId, AppResponse)> {
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
    ) -> Result<(PositionId, AppResponse)> {
        if self.id.get_market_type() == MarketType::CollateralIsBase {
            // let price = self.query_current_price()?;
            // let config = self.query_config()?;
            // println!("{} at price of {} (or {}) is {}", collateral, price.price, price.price.into_protocol_price(MarketType::CollateralIsBase), collateral * price.price.to_number());
            // let n:Number = leverage.try_into()?;
            // leverage = (n + Number::ONE).try_into()?;
            //direction = direction.invert();
        }

        let msg = self.token.into_market_execute_msg(
            &self.addr,
            Collateral::try_from_number(collateral)?,
            MarketExecuteMsg::OpenPosition {
                slippage_assert,
                leverage,
                direction,
                max_gains,
                stop_loss_override,
                take_profit_override,
            },
        )?;

        let res = self.exec_wasm_msg(sender, msg)?;
        let pos_id = res.event_first_value("position-open", "pos-id")?.parse()?;

        Ok((pos_id, res))
    }

    pub fn exec_close_position(
        &self,
        sender: &Addr,
        position_id: PositionId,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<AppResponse> {
        self.exec(
            sender,
            &MarketExecuteMsg::ClosePosition {
                id: position_id,
                slippage_assert,
            },
        )
    }

    pub fn exec_update_position_collateral_impact_leverage(
        &self,
        sender: &Addr,
        position_id: PositionId,
        collateral_delta: Signed<Collateral>,
    ) -> Result<AppResponse> {
        let msg = self.token.into_market_execute_msg(
            &self.addr,
            collateral_delta
                .try_into_positive_value()
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

        let res = self.exec_wasm_msg(sender, msg)?;

        Ok(res)
    }

    pub fn exec_update_position_collateral_impact_size(
        &self,
        sender: &Addr,
        position_id: PositionId,
        collateral_delta: Signed<Collateral>,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<AppResponse> {
        let msg = self.token.into_market_execute_msg(
            &self.addr,
            collateral_delta
                .try_into_positive_value()
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

        let res = self.exec_wasm_msg(sender, msg)?;

        Ok(res)
    }

    pub fn exec_update_position_leverage(
        &self,
        sender: &Addr,
        position_id: PositionId,
        leverage: LeverageToBase,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<AppResponse> {
        self.exec(
            sender,
            &MarketExecuteMsg::UpdatePositionLeverage {
                id: position_id,
                leverage,
                slippage_assert,
            },
        )
    }

    pub fn exec_update_position_max_gains(
        &self,
        sender: &Addr,
        position_id: PositionId,
        max_gains: MaxGainsInQuote,
    ) -> Result<AppResponse> {
        self.exec(
            sender,
            &MarketExecuteMsg::UpdatePositionMaxGains {
                id: position_id,
                max_gains,
            },
        )
    }

    pub fn exec_set_trigger_order(
        &self,
        sender: &Addr,
        position_id: PositionId,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<AppResponse> {
        self.exec(
            sender,
            &MarketExecuteMsg::SetTriggerOrder {
                id: position_id,
                stop_loss_override,
                take_profit_override,
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
    ) -> Result<(OrderId, AppResponse)> {
        let msg = self.token.into_market_execute_msg(
            &self.addr,
            collateral.raw(),
            ExecuteMsg::PlaceLimitOrder {
                trigger_price,
                leverage,
                direction,
                max_gains,
                stop_loss_override,
                take_profit_override,
            },
        )?;

        let res = self.exec_wasm_msg(sender, msg)?;
        let order_id = res
            .event_first_value(event_key::PLACE_LIMIT_ORDER, event_key::ORDER_ID)?
            .parse()?;

        Ok((order_id, res))
    }

    pub fn exec_cancel_limit_order(&self, sender: &Addr, order_id: OrderId) -> Result<AppResponse> {
        self.exec(sender, &MarketExecuteMsg::CancelLimitOrder { order_id })
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
            to_binary(msg)?,
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
            to_binary(msg)?,
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

    pub(crate) fn query_factory<T: DeserializeOwned>(&self, msg: &FactoryQueryMsg) -> Result<T> {
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
                on_exec(self.exec_refresh_price());
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

    /***** FARMING *****/
    pub fn rewards_token(&self) -> Token {
        self.app.borrow().rewards_token()
    }

    pub fn exec_farming_deposit_xlp(
        &self,
        wallet: &Addr,
        amount: NonZero<LpToken>,
    ) -> Result<AppResponse> {
        self.exec_liquidity_token_send(
            LiquidityTokenKind::Xlp,
            wallet,
            &self.farming_addr,
            amount.raw(),
            &FarmingExecuteMsg::Deposit {},
        )
    }

    pub fn exec_farming_deposit_collateral(
        &self,
        wallet: &Addr,
        amount: NonZero<Collateral>,
    ) -> Result<AppResponse> {
        let msg = self.token.into_execute_msg(
            &self.farming_addr.clone(),
            amount.raw(),
            &FarmingExecuteMsg::Deposit {},
        )?;

        self.exec_mint_tokens(wallet, amount.into_number())?;
        self.exec_wasm_msg(wallet, msg)
    }

    pub fn exec_farming_deposit_lp(
        &self,
        wallet: &Addr,
        amount: NonZero<LpToken>,
    ) -> Result<AppResponse> {
        self.exec_mint_and_deposit_liquidity(wallet, amount.into_number())
            .unwrap();
        self.exec_liquidity_token_send(
            LiquidityTokenKind::Lp,
            wallet,
            &self.farming_addr,
            amount.raw(),
            &FarmingExecuteMsg::Deposit {},
        )
    }

    pub fn exec_farming(&self, wallet: &Addr, msg: &FarmingExecuteMsg) -> Result<AppResponse> {
        self.exec_farming_with_funds(wallet, msg, vec![])
    }

    fn exec_farming_with_funds(
        &self,
        wallet: &Addr,
        msg: &FarmingExecuteMsg,
        funds: Vec<Coin>,
    ) -> Result<AppResponse> {
        let farming_addr = self.farming_addr.clone();

        let msg = WasmMsg::Execute {
            contract_addr: farming_addr.into_string(),
            msg: to_binary(msg)?,
            funds,
        };
        self.exec_wasm_msg(wallet, msg)
    }

    pub fn exec_farming_withdraw_xlp(
        &self,
        wallet: &Addr,
        amount: Option<NonZero<FarmingToken>>,
    ) -> Result<AppResponse> {
        self.exec_farming(wallet, &FarmingExecuteMsg::Withdraw { amount })
    }

    pub fn exec_farming_start_lockdrop(&self, start: Option<Timestamp>) -> Result<AppResponse> {
        self.exec_farming(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &FarmingExecuteMsg::Owner(FarmingOwnerExecuteMsg::StartLockdropPeriod { start }),
        )
    }

    pub fn exec_farming_start_launch(&self) -> Result<AppResponse> {
        self.exec_farming(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &FarmingExecuteMsg::Owner(FarmingOwnerExecuteMsg::StartLaunchPeriod {}),
        )
    }

    pub fn exec_farming_lockdrop_deposit(
        &self,
        wallet: &Addr,
        amount: NonZero<Collateral>,
        bucket_id: LockdropBucketId,
    ) -> Result<AppResponse> {
        let msg = self.token.into_execute_msg(
            &self.farming_addr,
            amount.raw(),
            &FarmingExecuteMsg::LockdropDeposit { bucket_id },
        )?;

        self.exec_wasm_msg(wallet, msg)
    }

    pub fn exec_farming_lockdrop_withdraw(
        &self,
        wallet: &Addr,
        amount: NonZero<Collateral>,
        bucket_id: LockdropBucketId,
    ) -> Result<AppResponse> {
        self.exec_farming(
            wallet,
            &FarmingExecuteMsg::LockdropWithdraw { amount, bucket_id },
        )
    }

    pub fn mint_lvn_rewards(&self, amount: &str, recipient: Option<Addr>) -> Token {
        let mut app = self.app();
        let token = app.rewards_token();
        let owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
        let recipient = recipient.unwrap_or(owner);

        app.mint_token(&recipient, &token, amount.parse().unwrap())
            .unwrap();

        token
    }

    pub fn exec_farming_set_lockdrop_rewards(
        &self,
        lvn_amount: NonZero<LvnToken>,
        lvn_token: &Token,
    ) -> Result<AppResponse> {
        let funds = NumberGtZero::try_from_decimal(lvn_amount.into_decimal256())
            .and_then(|amount| lvn_token.into_native_coin(amount).unwrap())
            .unwrap();

        self.exec_farming_with_funds(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &FarmingExecuteMsg::Owner(FarmingOwnerExecuteMsg::SetLockdropRewards {
                lvn: lvn_amount,
            }),
            vec![funds],
        )
    }

    pub fn exec_farming_set_emissions(
        &self,
        start: Timestamp,
        duration: u32,
        lvn_amount: NonZero<LvnToken>,
        lvn_token: Token,
    ) -> Result<AppResponse> {
        let funds = NumberGtZero::try_from_decimal(lvn_amount.into_decimal256())
            .and_then(|amount| lvn_token.into_native_coin(amount).unwrap())
            .unwrap();

        self.exec_farming_with_funds(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &FarmingExecuteMsg::Owner(FarmingOwnerExecuteMsg::SetEmissions {
                start: Some(start),
                duration,
                lvn: lvn_amount,
            }),
            vec![funds],
        )
    }

    pub fn exec_farming_clear_emissions(&self) -> Result<AppResponse> {
        self.exec_farming(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &FarmingExecuteMsg::Owner(FarmingOwnerExecuteMsg::ClearEmissions {}),
        )
    }

    pub fn exec_farming_reclaim_emissions(
        &self,
        addr: &Addr,
        amount: Option<LvnToken>,
    ) -> Result<AppResponse> {
        self.exec_farming(
            &Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &FarmingExecuteMsg::Owner(ReclaimEmissions {
                addr: addr.into(),
                amount,
            }),
        )
    }

    pub fn exec_farming_claim_lockdrop_rewards(&self, sender: &Addr) -> Result<AppResponse> {
        self.exec_farming(sender, &FarmingExecuteMsg::ClaimLockdropRewards {})
    }

    pub fn exec_farming_claim_emissions(&self, sender: &Addr) -> Result<AppResponse> {
        self.exec_farming(sender, &FarmingExecuteMsg::ClaimEmissions {})
    }

    pub fn exec_farming_reinvest(&self) -> Result<AppResponse> {
        self.exec_farming(&Addr::unchecked("user"), &FarmingExecuteMsg::Reinvest {})
    }

    pub fn exec_farming_transfer_bonus(&self) -> Result<AppResponse> {
        self.exec_farming(
            &Addr::unchecked("user"),
            &FarmingExecuteMsg::TransferBonus {},
        )
    }

    pub fn exec_farming_update_config(
        &self,
        owner: &Addr,
        new_owner: Option<RawAddr>,
        bonus_ratio: Option<Decimal256>,
        bonus_addr: Option<RawAddr>,
    ) -> Result<AppResponse> {
        self.exec_farming(
            owner,
            &FarmingExecuteMsg::Owner(OwnerExecuteMsg::UpdateConfig {
                owner: new_owner,
                bonus_ratio,
                bonus_addr,
            }),
        )
    }

    fn query_farming<T: DeserializeOwned>(
        &self,
        msg: &msg::contracts::farming::entry::QueryMsg,
    ) -> Result<T> {
        let farming_addr = self.farming_addr.clone();

        self.app()
            .wrap()
            .query_wasm_smart(farming_addr, &msg)
            .map_err(|err| err.into())
    }

    pub fn query_farmers(
        &self,
        start_after: Option<RawAddr>,
        limit: Option<u32>,
    ) -> Result<FarmersResp> {
        self.query_farming(&FarmingQueryMsg::Farmers { start_after, limit })
    }

    pub fn query_farming_farmer_stats(&self, wallet: &Addr) -> Result<FarmerStats> {
        self.query_farming(&FarmingQueryMsg::FarmerStats {
            addr: wallet.into(),
        })
    }

    pub fn query_farming_status(&self) -> FarmingStatusResp {
        self.query_farming(&FarmingQueryMsg::Status {}).unwrap()
    }

    pub fn query_reward_token_balance(&self, token: &Token, addr: &Addr) -> LvnToken {
        token
            .query_balance_dec(&self.app().querier(), addr)
            .map(LvnToken::from_decimal256)
            .unwrap()
    }
}
