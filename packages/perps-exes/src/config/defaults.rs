use cosmwasm_std::Decimal256;

use super::{GasAmount, TaskConfig};

pub(super) fn min_gas() -> GasAmount {
    "1".parse().unwrap()
}

pub(super) fn min_gas_in_faucet() -> GasAmount {
    "10000".parse().unwrap()
}

pub(super) fn min_gas_in_gas_wallet() -> GasAmount {
    "10000".parse().unwrap()
}

pub(super) fn retries() -> usize {
    6
}

pub(super) fn delay_between_retries() -> u32 {
    20
}

pub(super) fn balance() -> TaskConfig {
    super::WatcherConfig::default().balance
}

pub(super) fn gas_check() -> TaskConfig {
    super::WatcherConfig::default().gas_check
}

pub(super) fn liquidity() -> TaskConfig {
    super::WatcherConfig::default().liquidity
}

pub(super) fn trader() -> TaskConfig {
    super::WatcherConfig::default().trader
}

pub(super) fn utilization() -> TaskConfig {
    super::WatcherConfig::default().utilization
}

pub(super) fn track_balance() -> TaskConfig {
    super::WatcherConfig::default().track_balance
}

pub(super) fn crank_watch() -> TaskConfig {
    super::WatcherConfig::default().crank_watch
}

pub(super) fn crank_run() -> TaskConfig {
    super::WatcherConfig::default().crank_run
}

pub(super) fn get_factory() -> TaskConfig {
    super::WatcherConfig::default().get_factory
}

pub(super) fn price() -> TaskConfig {
    super::WatcherConfig::default().price
}

pub(super) fn stale() -> TaskConfig {
    super::WatcherConfig::default().stale
}

pub(super) fn stats() -> TaskConfig {
    super::WatcherConfig::default().stats
}

pub(super) fn stats_alert() -> TaskConfig {
    super::WatcherConfig::default().stats_alert
}

pub(super) fn ultra_crank() -> TaskConfig {
    super::WatcherConfig::default().ultra_crank
}

pub(super) fn liquidity_transaction_alert() -> TaskConfig {
    super::WatcherConfig::default().liquidity_transaction
}

pub(super) fn rpc_health() -> TaskConfig {
    super::WatcherConfig::default().rpc_health
}

pub(super) fn seconds_till_ultra() -> u32 {
    // 8 minutes
    60 * 8
}

pub fn max_price_age_secs() -> u32 {
    // 1 hour
    60 * 60
}

pub fn max_allowed_price_delta() -> Decimal256 {
    "0.01".parse().unwrap()
}
