use std::collections::{HashMap, HashSet};

use super::state::*;
use crate::{
    page::home::app::{ActionHistory, ExecErrorEvent, TimeJumpEvent, TxEvent},
    prelude::*,
};
use cosmwasm_std::Event as CosmosEvent;
use msg::{
    bridge::ExecError,
    constants::event_key,
    contracts::market::{
        config::Config as MarketConfig,
        entry::{ExecuteMsg, QueryMsg, StatusResp, TradeHistorySummary},
        position::{
            events::{PositionCloseEvent, PositionOpenEvent, PositionUpdateEvent},
            PositionId,
        },
        spot_price::events::SpotPriceEvent,
    },
};

pub struct EventUpdateResult {
    pub open_positions: Vec<PositionId>,
    pub closed_positions: Vec<PositionId>,
    pub updated_positions: Vec<PositionId>,
    pub prices: Vec<PriceBaseInQuote>,
    pub deposit_collateral: Option<Signed<Collateral>>,
}

impl EventUpdateResult {
    pub fn new() -> Self {
        Self {
            open_positions: vec![],
            closed_positions: vec![],
            updated_positions: vec![],
            prices: vec![],
            deposit_collateral: None,
        }
    }
}

impl Stats {
    pub fn update_events(
        &self,
        msg_id: u64,
        msg_elapsed: f64,
        execute_msg: ExecuteMsg,
        events: Vec<CosmosEvent>,
    ) -> Result<EventUpdateResult> {
        let mut res = EventUpdateResult::new();

        // first parse all the events to accummulate the changes
        for event in events.iter() {
            if event.ty.starts_with("wasm-position-open") {
                let evt: PositionOpenEvent = event.clone().try_into().unwrap();
                res.open_positions.push(evt.position_attributes.pos_id);
                let value = evt.position_attributes.collaterals.deposit_collateral;
                res.deposit_collateral = Some(
                    res.deposit_collateral
                        .map_or(Ok(value), |prev| prev + value)?,
                );
            } else if event.ty.starts_with("wasm-position-close") {
                let evt: PositionCloseEvent = event.clone().try_into().unwrap();
                res.closed_positions.push(evt.closed_position.id);
                let value = evt.closed_position.deposit_collateral;
                res.deposit_collateral = Some(
                    -res.deposit_collateral
                        .map_or(Ok(value), |prev| prev - value)?,
                );
            } else if event.ty.starts_with("wasm-position-update") {
                let evt: PositionUpdateEvent = event.clone().try_into().unwrap();
                let value = evt.deposit_collateral_delta;
                res.deposit_collateral = Some(
                    -res.deposit_collateral
                        .map_or(Ok(value), |prev| prev - value)?,
                );
            } else if event.ty.starts_with("wasm-spot-price") {
                let evt: SpotPriceEvent = event.clone().try_into().unwrap();
                res.prices.push(evt.price_base);
            }
        }

        // positions
        let mut lock = self.position_ids.lock_mut();
        for pos_id in &res.open_positions {
            lock.insert(*pos_id);
        }
        for pos_id in &res.closed_positions {
            lock.remove(pos_id);
        }

        // deposit collateral
        if let Some(collateral) = res.deposit_collateral {
            let mut lock = self.deposit_collateral.lock_mut();
            if let Ok(new) = *lock + collateral {
                *lock = new;
            }
        }

        // price
        if let Some(price) = res.prices.last() {
            self.price.set_neq(Some(*price));
        }

        // tx events
        self.push_action_history(ActionHistory::Tx(TxEvent {
            msg_id,
            msg_elapsed,
            execute_msg,
            events,
        }));

        Ok(res)
    }

    pub fn update_time_jump(&self, msg_id: u64, msg_elapsed: f64, seconds: i64) {
        self.push_action_history(ActionHistory::TimeJump(TimeJumpEvent {
            msg_id,
            msg_elapsed,
            seconds,
        }));
    }

    pub fn update_trade_history(&self, _msg_id: u64, _msg_elapsed: f64, data: TradeHistorySummary) {
        self.trade_volume.set_neq(data.trade_volume);
        self.realized_pnl.set_neq(data.realized_pnl);
    }

    pub fn update_market_status(&self, _msg_id: u64, _msg_elapsed: f64, data: StatusResp) {
        self.market_status.set(Some(data));
    }

    pub fn update_error(&self, error: anyhow::Error) {
        let error = match error.downcast_ref::<PerpError>() {
            None => ExecError::Unknown(error.to_string()),
            Some(error) => ExecError::PerpError(error.clone()),
        };

        self.push_action_history(ActionHistory::Error(ExecErrorEvent { error }));
    }

    fn push_action_history(&self, action: ActionHistory) {
        let mut lock = self.action_history.lock_mut();
        lock.insert_cloned(0, action);
        if lock.len() > CONFIG.max_stats_backlog {
            lock.pop();
        }
    }
}
