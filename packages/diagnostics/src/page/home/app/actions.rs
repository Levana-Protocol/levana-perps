use super::{controls::PlayState, state::*, stats::EventUpdateResult};
use crate::{
    prelude::*,
    runner::exec::{Action, ActionContext, ActionLog},
};
use awsm_web::{loaders::helpers::spawn_handle, tick::Timestamp};
use cosmwasm_std::Event as CosmosEvent;
use msg::{
    constants::event_key,
    contracts::market::{
        config::Config as MarketConfig,
        entry::{ExecuteMsg, QueryMsg, StatusResp, TradeHistorySummary},
        position::{events::PositionOpenEvent, PositionId},
    },
};
use rand::{
    distributions::{uniform::SampleRange, Standard},
    prelude::*,
};
use std::{
    borrow::BorrowMut,
    collections::HashSet,
    ops::{Div, Mul, Range, RangeInclusive, Sub},
    sync::{atomic::Ordering, Arc},
};

impl App {
    pub fn on_timestamp(self: Rc<Self>, timestamp: Timestamp) {
        let state = self;
        let countdown = *state.next_action_countdown.borrow();

        if let Some(countdown) = countdown {
            if countdown <= 0.0 {
                *state.action_handle.borrow_mut() = Some(spawn_handle(
                    clone!(state => async move {
                        *state.next_action_countdown.borrow_mut() = None;
                        state.do_action().await;
                        if state.controls.play_state.get() == PlayState::Play {
                            *state.next_action_countdown.borrow_mut() = Some(state.controls.delay.borrow().clone());
                        }
                    }),
                ));
            } else {
                let delta_seconds = timestamp.delta / 1000.0;
                *state.next_action_countdown.borrow_mut() = Some(countdown - delta_seconds);
            }
        }
    }

    pub fn play(&self) {
        self.controls.play_state.set_neq(PlayState::Play);
        *self.next_action_countdown.borrow_mut() = Some(0.0);
    }

    pub fn pause(&self) {
        self.controls.play_state.set_neq(PlayState::Pause);
        *self.next_action_countdown.borrow_mut() = None;
        // need to let the previous action complete
        // otherwise the bridge will try to send
        // and there will be no receiver
        // also, we'll be missing some data
        // i.e. do NOT do this:
        // *self.action_handle.borrow_mut() = None;
    }

    pub async fn do_action(&self) {
        let mut actions = Action::all().to_vec();
        actions.retain(|action| self.controls.get_allow_action(*action));

        let action: Option<Action> = if actions.is_empty() {
            None
        } else {
            Some(
                actions
                    .choose(&mut *self.rng.borrow_mut())
                    .cloned()
                    .unwrap(),
            )
        };

        let mut ctx = ActionContext {
            market_type: self.market_type,
            market_config: &self.market_config,
            market_collateral_token: self.market_collateral_token.clone(),
            bridge: &self.bridge,
            rng: &mut *self.rng.borrow_mut(),
            get_open_positions: || {
                let lock: &HashSet<PositionId> = &*self.stats.position_ids.lock_ref();
                lock.iter().cloned().collect()
            },
            on_log: |action_log| match action_log {
                ActionLog::Execute {
                    msg_id,
                    msg_elapsed,
                    exec,
                    events,
                } => {
                    self.stats.update_events(msg_id, msg_elapsed, exec, events);
                }
                ActionLog::TimeJumpSeconds {
                    msg_id,
                    msg_elapsed,
                    seconds,
                } => {
                    self.stats.update_time_jump(msg_id, msg_elapsed, seconds);
                }
            },
        };

        // always refresh price
        if let Err(err) = self.bridge.refresh_price().await {
            self.stats.update_error(err);
        }

        if let Err(err) = ctx.do_action(action, self.controls.get_allow_crank()).await {
            self.stats.update_error(err);
        }

        let resp = self
            .bridge
            .query_market(QueryMsg::TradeHistorySummary {
                addr: (&CONFIG.user_addr).into(),
            })
            .await
            .unwrap();
        self.stats
            .update_trade_history(resp.msg_id, resp.msg_elapsed, resp.data);

        let resp = self
            .bridge
            .query_market(QueryMsg::Status { price: None })
            .await
            .unwrap();
        self.stats
            .update_market_status(resp.msg_id, resp.msg_elapsed, resp.data);
    }
}
