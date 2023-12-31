#![allow(missing_docs)]
use super::MaxLiquidity;
use cosmwasm_std::Decimal256;
use shared::storage::{NonZero, Number, NumberGtZero, Usd};

pub struct ConfigDefaults {}

impl ConfigDefaults {
    pub fn trading_fee_notional_size() -> Decimal256 {
        "0.001".parse().unwrap()
    }

    pub fn trading_fee_counter_collateral() -> Decimal256 {
        "0.001".parse().unwrap()
    }
    pub const fn crank_execs() -> u32 {
        7
    }
    pub fn max_leverage() -> Number {
        Number::try_from("30").unwrap()
    }
    pub fn funding_rate_sensitivity() -> Decimal256 {
        "10".parse().unwrap()
    }
    pub fn funding_rate_max_annualized() -> Decimal256 {
        "0.9".parse().unwrap()
    }
    pub fn borrow_fee_rate_min_annualized() -> NumberGtZero {
        "0.01".parse().unwrap()
    }
    pub fn borrow_fee_rate_max_annualized() -> NumberGtZero {
        "0.60".parse().unwrap()
    }
    pub fn carry_leverage() -> Decimal256 {
        "10".parse().unwrap()
    }
    pub const fn mute_events() -> bool {
        false
    }
    pub const fn liquifunding_delay_seconds() -> u32 {
        60 * 60 * 6
    }
    pub fn protocol_tax() -> Decimal256 {
        "0.3".parse().unwrap()
    }
    pub const fn unstake_period_seconds() -> u32 {
        // 45 day
        60 * 60 * 24 * 45
    }
    pub fn target_utilization() -> NonZero<Decimal256> {
        "0.8".parse().unwrap()
    }

    pub fn borrow_fee_sensitivity() -> NumberGtZero {
        // Try to realize the bias over a 3 day period.
        //
        // See: https://phobosfinance.atlassian.net/browse/PERP-606
        //
        // Spreadsheet calculated this value:
        //
        // https://docs.google.com/spreadsheets/d/15EG3I6XnaUKI20ja7XiCqLOjFS80QhKdnoL-PsjzJ-0/edit#gid=0
        NumberGtZero::try_from(Number::ONE / Number::try_from("12").unwrap()).unwrap()
    }

    pub fn max_xlp_rewards_multiplier() -> NumberGtZero {
        "2".parse().unwrap()
    }
    pub fn min_xlp_rewards_multiplier() -> NumberGtZero {
        "1".parse().unwrap()
    }
    pub fn delta_neutrality_fee_sensitivity() -> NumberGtZero {
        "50000000".parse().unwrap()
    }
    pub fn delta_neutrality_fee_cap() -> NumberGtZero {
        "0.005".parse().unwrap()
    }
    pub fn delta_neutrality_fee_tax() -> Decimal256 {
        "0.05".parse().unwrap()
    }
    pub fn crank_fee_charged() -> Usd {
        "0.01".parse().unwrap()
    }
    pub fn crank_fee_surcharge() -> Usd {
        "0.005".parse().unwrap()
    }
    pub fn crank_fee_reward() -> Usd {
        "0.005".parse().unwrap()
    }
    pub fn minimum_deposit_usd() -> Usd {
        "5".parse().unwrap()
    }

    pub const fn liquifunding_delay_fuzz_seconds() -> u32 {
        60 * 60
    }
    pub const fn max_liquidity() -> MaxLiquidity {
        MaxLiquidity::Unlimited {}
    }
    pub const fn disable_position_nft_exec() -> bool {
        false
    }
    pub const fn liquidity_cooldown_seconds() -> u32 {
        // FIXME in the future, bump to 6.5 hours. Holding off for now because
        // otherwise we'll get confusing messages from sync-config. Wait until
        // we're about to migrate contracts again.

        // Default to 1 hour
        60 * 60
    }
}
