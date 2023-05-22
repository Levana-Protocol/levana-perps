use super::TaskConfig;

pub(super) fn min_gas() -> u128 {
    1_000_000
}

pub(super) fn min_gas_in_faucet() -> u128 {
    10_000_000_000
}

pub(super) fn min_gas_in_gas_wallet() -> u128 {
    10_000_000_000
}

pub(super) fn retries() -> usize {
    4
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

pub(super) fn crank() -> TaskConfig {
    super::WatcherConfig::default().crank
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

pub(super) fn ultra_crank() -> TaskConfig {
    super::WatcherConfig::default().ultra_crank
}

pub(super) fn seconds_till_ultra() -> u32 {
    // 8 minutes
    60 * 8
}
