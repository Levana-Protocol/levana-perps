use crate::{prelude::*, runner::exec::Action};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Controls {
    pub play_state: Mutable<PlayState>,
    pub delay: RefCell<f64>,
    pub allow_open: AtomicBool,
    pub allow_close: AtomicBool,
    pub allow_update: AtomicBool,
    pub allow_price: AtomicBool,
    pub allow_time_jump: AtomicBool,
    pub allow_crank: AtomicBool,
    pub show_graph: Mutable<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayState {
    Play,
    Pause,
}

impl Controls {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            play_state: Mutable::new(CONFIG.play_state),
            delay: RefCell::new(CONFIG.action_delay),
            allow_open: AtomicBool::new(CONFIG.allowed_actions.contains(&Action::OpenPosition)),
            allow_close: AtomicBool::new(CONFIG.allowed_actions.contains(&Action::ClosePosition)),
            allow_update: AtomicBool::new(CONFIG.allowed_actions.contains(&Action::UpdatePosition)),
            allow_price: AtomicBool::new(CONFIG.allowed_actions.contains(&Action::SetPrice)),
            allow_time_jump: AtomicBool::new(CONFIG.allowed_actions.contains(&Action::TimeJump)),
            allow_crank: AtomicBool::new(CONFIG.crank),
            show_graph: Mutable::new(CONFIG.show_graph),
        })
    }

    pub fn play_signal(&self) -> impl Signal<Item = PlayState> {
        self.play_state.signal()
    }

    pub fn get_allow_action(&self, action: Action) -> bool {
        match action {
            Action::OpenPosition => self.allow_open.load(Ordering::SeqCst),
            Action::ClosePosition => self.allow_close.load(Ordering::SeqCst),
            Action::UpdatePosition => self.allow_update.load(Ordering::SeqCst),
            Action::SetPrice => self.allow_price.load(Ordering::SeqCst),
            Action::TimeJump => self.allow_time_jump.load(Ordering::SeqCst),
        }
    }
    pub fn get_allow_crank(&self) -> bool {
        self.allow_crank.load(Ordering::SeqCst)
    }
}

impl PlayState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Pause => "Pause",
        }
    }

    pub fn inverse(&self) -> Self {
        match self {
            Self::Play => Self::Pause,
            Self::Pause => Self::Play,
        }
    }
}
