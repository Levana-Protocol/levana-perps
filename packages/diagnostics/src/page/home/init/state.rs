use crate::{bridge::Bridge, prelude::*};
use dominator_helpers::futures::AsyncLoader;
use futures_signals::signal::option;
use msg::{
    contracts::market::{config::Config as MarketConfig, entry::StatusResp},
    token::Token,
};
use web_sys::WebGl2RenderingContext;

pub struct InitUi {
    pub phase: Mutable<Phase>,
}

impl InitUi {
    pub fn new() -> Rc<Self> {
        let _self = Rc::new(Self {
            phase: Mutable::new(Phase::Disconnected),
        });

        if CONFIG.auto_connect_bridge {
            _self.clone().connect();
        }

        _self
    }
}

#[derive(Clone)]
pub enum Phase {
    Disconnected,
    Connecting,
    Error(String),
    Connected {
        bridge: Rc<Bridge>,
        status: StatusResp,
    },
}
