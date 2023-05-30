//! Functions providing serde defaults

use cosmwasm_std::Decimal256;
use shared::storage::NonZero;

use crate::contracts::farming::entry::LockdropBucketId;

use super::LockdropBucketConfig;

/// Default number of seconds in a lockdrop month
pub fn lockdrop_month_seconds() -> u32 {
    86400
}

/// Default buckets for the lockdrop
pub fn lockdrop_buckets() -> Vec<LockdropBucketConfig> {
    fn go(months: u32, multiplier: &str) -> LockdropBucketConfig {
        LockdropBucketConfig {
            bucket_id: LockdropBucketId(months),
            multiplier: multiplier.parse().unwrap(),
        }
    }
    vec![
        go(3, "1"),
        go(6, "2.8"),
        go(9, "5.2"),
        go(12, "8"),
        go(15, "11.2"),
        go(18, "14.7"),
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
