use awsm_web::{env::env_var, prelude::UnwrapExt};
use cosmwasm_std::Addr;
use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::{page::home::app::controls::PlayState, runner::exec::Action};

cfg_if::cfg_if! {
    if #[cfg(feature = "dev")] {
        pub const CONFIG: Lazy<Config> = Lazy::new(|| {
            Config {
                uri_root: "",
                bridge_addr: "ws://127.0.0.1:31337",
                auto_connect_bridge: true,
                play_state: PlayState::Pause,
                action_delay: 0.1,
                max_stats_backlog: 100,
                user_addr: Addr::unchecked("user"),
                price_admin_addr: Addr::unchecked("price-admin"),
                protocol_owner_addr: Addr::unchecked("protocol-owner"),
                allowed_actions: vec![Action::OpenPosition, Action::ClosePosition, Action::UpdatePosition, Action::SetPrice, Action::TimeJump],
                crank: true,
                show_graph: true,
            }
        });
    } else {
        pub const CONFIG: Lazy<Config> = Lazy::new(|| {
            Config {
                uri_root: "",
                bridge_addr: "ws://127.0.0.1:31337",
                auto_connect_bridge: false,
                play_state: PlayState::Pause,
                action_delay: 0.1,
                max_stats_backlog: 100,
                user_addr: Addr::unchecked("user"),
                price_admin_addr: Addr::unchecked("price-admin"),
                protocol_owner_addr: Addr::unchecked("protocol-owner"),
                allowed_actions: vec![Action::OpenPosition, Action::ClosePosition, Action::UpdatePosition, Action::SetPrice, Action::TimeJump],
                crank: true,
                show_graph: true,
            }
        });
    }
}

#[derive(Debug)]
pub struct Config {
    pub uri_root: &'static str,
    pub bridge_addr: &'static str,
    pub auto_connect_bridge: bool,
    pub play_state: PlayState,
    pub action_delay: f64,
    pub max_stats_backlog: usize,
    pub user_addr: Addr,
    pub price_admin_addr: Addr,
    pub protocol_owner_addr: Addr,
    pub allowed_actions: Vec<Action>,
    pub crank: bool,
    pub show_graph: bool,
}

fn check_env(name: &str) -> Option<String> {
    match env_var(name) {
        Ok(value) => {
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        }
        Err(_) => None,
    }
}
