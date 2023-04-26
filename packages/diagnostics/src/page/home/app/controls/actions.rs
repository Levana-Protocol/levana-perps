use super::state::*;
use crate::{prelude::*, runner::exec::Action};
use std::sync::atomic::Ordering;

impl Controls {
    pub fn set_allow_action(&self, action: Action, flag: bool) {
        match action {
            Action::OpenPosition => self.allow_open.store(flag, Ordering::SeqCst),
            Action::UpdatePosition => self.allow_update.store(flag, Ordering::SeqCst),
            Action::ClosePosition => self.allow_close.store(flag, Ordering::SeqCst),
            Action::SetPrice => self.allow_price.store(flag, Ordering::SeqCst),
            Action::TimeJump => self.allow_time_jump.store(flag, Ordering::SeqCst),
        }
    }

    pub fn set_allow_crank(&self, flag: bool) {
        self.allow_crank.store(flag, Ordering::SeqCst);
    }
}
