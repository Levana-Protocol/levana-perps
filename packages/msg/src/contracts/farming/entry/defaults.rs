//! Functions providing serde defaults

use cosmwasm_std::Decimal256;
use shared::storage::NonZero;

use crate::contracts::farming::entry::LockdropBucket;

use super::LockdropBucketConfig;

/// Default number of seconds in a lockdrop month
pub fn lockdrop_month_seconds() -> u32 {
    86400
}

/// Default buckets for the lockdrop
pub fn lockdrop_buckets() -> Vec<LockdropBucketConfig> {
    fn go(months: u32, multiplier: &str) -> LockdropBucketConfig {
        LockdropBucketConfig {
            bucket: LockdropBucket(months),
            multiplier: multiplier.parse().unwrap(),
        }
    }
    vec![
        go(1, "1"),
        go(3, "3.3"),
        go(6, "7.8"),
        go(9, "12"),
        go(12, "17"),
        go(18, "24"),
    ]
}

/// Default bonus ratio
pub fn bonus_ratio() -> NonZero<Decimal256> {
    "0.05".parse().unwrap()
}

/// Default immediate unlock ratio
pub fn lockdrop_immediate_unlock_ratio() -> Decimal256 {
    "0.25".parse().unwrap()
}
