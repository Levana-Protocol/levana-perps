use super::{controls::Controls, event_view::EventView, graph::Graph, stats::Stats};
use crate::{prelude::*, runner::exec::Action};
use awsm_web::{loaders::helpers::FutureHandle, tick::TimestampLoop};
use cosmwasm_std::Event as CosmosEvent;
use msg::{
    bridge::ExecError,
    contracts::market::{
        config::Config as MarketConfig,
        entry::{ExecuteMsg, StatusResp, TradeHistorySummary},
        position::PositionId,
    },
    prelude::*,
    token::Token,
};
use rand::prelude::*;
use std::{
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};
use wasm_bindgen_futures::spawn_local;

pub struct App {
    pub bridge: Rc<Bridge>,
    pub market_id: MarketId,
    pub market_type: MarketType,
    pub market_collateral_token: Token,
    pub market_config: MarketConfig,
    pub controls: Rc<Controls>,
    pub timestamp_loop: RefCell<Option<TimestampLoop>>,
    pub next_action_countdown: RefCell<Option<f64>>,
    pub action_handle: RefCell<Option<FutureHandle>>,
    pub graph: Rc<Graph>,
    pub event_view: Rc<EventView>,
    pub stats: Rc<Stats>,
    pub rng: RefCell<rand::rngs::ThreadRng>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ActionHistory {
    Tx(TxEvent),
    TimeJump(TimeJumpEvent),
    Error(ExecErrorEvent),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxEvent {
    pub msg_id: u64,
    pub msg_elapsed: f64,
    pub execute_msg: ExecuteMsg,
    pub events: Vec<CosmosEvent>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimeJumpEvent {
    pub msg_id: u64,
    pub msg_elapsed: f64,
    pub seconds: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecErrorEvent {
    pub error: ExecError,
}

impl App {
    pub fn new(bridge: Rc<Bridge>, status: StatusResp) -> Rc<Self> {
        let stats = Stats::new(bridge.clone());
        let controls = Controls::new();

        let market_collateral_token = status.collateral;
        let market_config = status.config;
        let market_id = status.market_id;
        let market_type = status.market_type;

        let _self = Rc::new(Self {
            bridge: bridge.clone(),
            market_collateral_token: market_collateral_token.clone(),
            market_config: market_config.clone(),
            market_id: market_id.clone(),
            market_type: market_type.clone(),
            controls: controls.clone(),
            timestamp_loop: RefCell::new(None),
            next_action_countdown: RefCell::new(None),
            action_handle: RefCell::new(None),
            graph: Graph::new(
                bridge,
                market_id,
                market_type,
                market_collateral_token,
                market_config,
                stats.clone(),
                controls.clone(),
            ),
            event_view: EventView::new(stats.clone()),
            stats,
            rng: RefCell::new(rand::thread_rng()),
        });

        *_self.timestamp_loop.borrow_mut() = Some(
            TimestampLoop::start(clone!(_self => move |timestamp| {
                _self.clone().on_timestamp(timestamp);
            }))
            .unwrap(),
        );

        _self
    }
}
