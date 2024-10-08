//! Market-wide configuration

pub mod defaults;
use crate::prelude::*;

use self::defaults::ConfigDefaults;

use super::spot_price::{SpotPriceConfig, SpotPriceConfigInit};

/// Configuration info for the vAMM
/// Set by admin-only

/// Since this tends to cross the message boundary
/// all the numeric types are u32 or lower
/// helper functions are available where more bits are needed
#[cw_serde]
pub struct Config {
    /// The fee to open a position, as a percentage of the notional size
    pub trading_fee_notional_size: Decimal256,
    /// The fee to open a position, as a percentage of the counter-side collateral
    pub trading_fee_counter_collateral: Decimal256,
    /// default number of crank exeuctions to do when none specified
    pub crank_execs: u32,
    /// The maximum allowed leverage when opening a position
    pub max_leverage: Number,
    /// Impacts how much the funding rate changes in response to net notional changes.
    pub funding_rate_sensitivity: Decimal256,
    /// The maximum annualized rate for a funding payment
    pub funding_rate_max_annualized: Decimal256,
    /// The minimum annualized rate for borrow fee payments
    pub borrow_fee_rate_min_annualized: NumberGtZero,
    /// The maximum annualized rate for borrow fee payments
    pub borrow_fee_rate_max_annualized: NumberGtZero,
    /// Needed to ensure financial model is balanced
    ///
    /// Must be at most 1 less than the [Config::max_leverage]
    pub carry_leverage: Decimal256,
    /// Do not emit events (default is false, events *will* be emitted)
    pub mute_events: bool,
    /// Delay between liquifundings, in seconds
    pub liquifunding_delay_seconds: u32,
    /// The percentage of fees that are taken for the protocol
    pub protocol_tax: Decimal256,
    /// How long it takes to unstake xLP tokens into LP tokens, in seconds
    pub unstake_period_seconds: u32,
    /// Target utilization ratio liquidity, given as a ratio. (Must be between 0 and 1).
    pub target_utilization: NonZero<Decimal256>,
    /// Borrow fee sensitivity parameter.
    ///
    /// See [section 5.5 of the whitepaper](https://www.notion.so/levana-protocol/Levana-Well-funded-Perpetuals-Whitepaper-9805a6eba56d429b839f5551dbb65c40#295f9f2689e74ccab16ca28177eb32cb).
    pub borrow_fee_sensitivity: NumberGtZero,
    /// Maximum multiplier for xLP versus LP borrow fee shares.
    ///
    /// For example, if this number is 5, then as liquidity in the protocol
    /// approaches 100% in LP and 0% in xLP, any xLP token will receive 5x the
    /// rewards of an LP token.
    pub max_xlp_rewards_multiplier: NumberGtZero,
    /// Minimum counterpoint to [Config::max_xlp_rewards_multiplier]
    pub min_xlp_rewards_multiplier: NumberGtZero,
    /// Delta neutrality fee sensitivity parameter.
    ///
    /// Higher values indicate markets with greater depth of liquidity, and allow for
    /// larger divergence for delta neutrality in the markets.
    ///
    /// This value is specified in the notional asset.
    pub delta_neutrality_fee_sensitivity: NumberGtZero,
    /// Delta neutrality fee cap parameter, given as a percentage
    pub delta_neutrality_fee_cap: NumberGtZero,
    /// Proportion of delta neutrality inflows that are sent to the protocol.
    pub delta_neutrality_fee_tax: Decimal256,
    /// The crank fee to be paid into the system, in collateral
    pub crank_fee_charged: Usd,
    /// The crank surcharge charged for every 10 items in the deferred execution queue.
    ///
    /// This is intended to create backpressure in times of high congestion.
    ///
    /// For every 10 items in the deferred execution queue, this amount is added to the
    /// crank fee charged on performing a deferred execution message.
    ///
    /// This is only charged while adding new items to the queue, not when performing
    /// ongoing tasks like liquifunding or liquidations.
    #[serde(default = "ConfigDefaults::crank_fee_surcharge")]
    pub crank_fee_surcharge: Usd,
    /// The crank fee to be sent to crankers, in collateral
    pub crank_fee_reward: Usd,
    /// Minimum deposit collateral, given in USD
    pub minimum_deposit_usd: Usd,
    /// The liquifunding delay fuzz factor, in seconds.
    ///
    /// Up to how many seconds will we perform a liquifunding early. This will
    /// be part of a semi-randomly generated value and will allow us to schedule
    /// liquifundings arbitrarily to smooth out spikes in traffic.
    #[serde(default = "ConfigDefaults::liquifunding_delay_fuzz_seconds")]
    pub liquifunding_delay_fuzz_seconds: u32,
    /// The maximum amount of liquidity that can be deposited into the market.
    #[serde(default)]
    pub max_liquidity: MaxLiquidity,
    /// Disable the ability to proxy CW721 execution messages for positions.
    /// Even if this is true, queries will still work as usual.
    #[serde(default)]
    pub disable_position_nft_exec: bool,
    /// The liquidity cooldown period.
    ///
    /// After depositing new funds into the market, liquidity providers will
    /// have a period of time where they cannot withdraw their funds. This is
    /// intended to prevent an MEV attack where someone can reorder transactions
    /// to extract fees from traders without taking on any impairment risk.
    ///
    /// This protection is only triggered by deposit of new funds; reinvesting
    /// existing yield does not introduce a cooldown.
    ///
    /// While the cooldown is in place, providers are prevented from either
    /// withdrawing liquidity or transferring their LP and xLP tokens.
    ///
    /// For migration purposes, this value defaults to 0, meaning no cooldown period.
    #[serde(default)]
    pub liquidity_cooldown_seconds: u32,

    /// Ratio of notional size used for the exposure component of the liquidation margin.
    #[serde(default = "ConfigDefaults::exposure_margin_ratio")]
    pub exposure_margin_ratio: Decimal256,

    /// Portion of trading fees given as rewards to referrers.
    #[serde(default = "ConfigDefaults::referral_reward_ratio")]
    pub referral_reward_ratio: Decimal256,

    /// The spot price config for this market
    pub spot_price: SpotPriceConfig,

    // Fields below here are no longer used by the protocol, but kept in the data structure to ease migration.
    /// Just for historical reasons/migrations
    #[serde(rename = "price_update_too_old_seconds")]
    pub _unused1: Option<u32>,
    /// Just for historical reasons/migrations
    #[serde(rename = "unpend_limit")]
    pub _unused2: Option<u32>,
    /// Just for historical reasons/migrations
    #[serde(rename = "limit_order_fee")]
    pub _unused3: Option<Collateral>,
    /// Just for historical reasons/migrations
    #[serde(rename = "staleness_seconds")]
    pub _unused4: Option<u32>,
}

/// Maximum liquidity for deposit.
///
/// Note that this limit can be exceeded due to changes in collateral asset
/// price or impairment.
#[cw_serde]
pub enum MaxLiquidity {
    /// No bounds on how much liquidity can be deposited.
    Unlimited {},
    /// Only allow the given amount in USD.
    ///
    /// The exchange rate at time of deposit will be used.
    Usd {
        /// Amount in USD
        amount: NonZero<Usd>,
    },
}

impl Default for MaxLiquidity {
    fn default() -> Self {
        MaxLiquidity::Unlimited {}
    }
}

impl Config {
    /// create a new config with default values and a given spot price config
    pub fn new(spot_price: SpotPriceConfig) -> Self {
        // these unwraps are fine since we define the value
        Self {
            trading_fee_notional_size: ConfigDefaults::trading_fee_notional_size(),
            trading_fee_counter_collateral: ConfigDefaults::trading_fee_counter_collateral(),
            crank_execs: ConfigDefaults::crank_execs(),
            max_leverage: ConfigDefaults::max_leverage(),
            carry_leverage: ConfigDefaults::carry_leverage(),
            funding_rate_max_annualized: ConfigDefaults::funding_rate_max_annualized(),
            borrow_fee_rate_min_annualized: ConfigDefaults::borrow_fee_rate_min_annualized(),
            borrow_fee_rate_max_annualized: ConfigDefaults::borrow_fee_rate_max_annualized(),
            funding_rate_sensitivity: ConfigDefaults::funding_rate_sensitivity(),
            mute_events: ConfigDefaults::mute_events(),
            liquifunding_delay_seconds: ConfigDefaults::liquifunding_delay_seconds(),
            protocol_tax: ConfigDefaults::protocol_tax(),
            unstake_period_seconds: ConfigDefaults::unstake_period_seconds(),
            target_utilization: ConfigDefaults::target_utilization(),
            borrow_fee_sensitivity: ConfigDefaults::borrow_fee_sensitivity(),
            max_xlp_rewards_multiplier: ConfigDefaults::max_xlp_rewards_multiplier(),
            min_xlp_rewards_multiplier: ConfigDefaults::min_xlp_rewards_multiplier(),
            delta_neutrality_fee_sensitivity: ConfigDefaults::delta_neutrality_fee_sensitivity(),
            delta_neutrality_fee_cap: ConfigDefaults::delta_neutrality_fee_cap(),
            delta_neutrality_fee_tax: ConfigDefaults::delta_neutrality_fee_tax(),
            crank_fee_charged: ConfigDefaults::crank_fee_charged(),
            crank_fee_surcharge: ConfigDefaults::crank_fee_surcharge(),
            crank_fee_reward: ConfigDefaults::crank_fee_reward(),
            minimum_deposit_usd: ConfigDefaults::minimum_deposit_usd(),
            liquifunding_delay_fuzz_seconds: ConfigDefaults::liquifunding_delay_fuzz_seconds(),
            max_liquidity: ConfigDefaults::max_liquidity(),
            disable_position_nft_exec: ConfigDefaults::disable_position_nft_exec(),
            liquidity_cooldown_seconds: ConfigDefaults::liquidity_cooldown_seconds(),
            exposure_margin_ratio: ConfigDefaults::exposure_margin_ratio(),
            referral_reward_ratio: ConfigDefaults::referral_reward_ratio(),
            spot_price,
            _unused1: None,
            _unused2: None,
            _unused3: None,
            _unused4: None,
        }
    }

    /// Ensure that the settings within this [Config] are valid.
    pub fn validate(&self) -> Result<()> {
        // note - crank_execs_after_push and mute_events are inherently always valid

        if self.trading_fee_notional_size >= "0.0999".parse().unwrap() {
            perp_bail!(ErrorId::Config, ErrorDomain::Market, "trading_fee_notional_size must be in the range 0 to 0.0999 inclusive ({} is invalid)", self.trading_fee_notional_size );
        }

        if self.trading_fee_counter_collateral >= "0.0999".parse().unwrap() {
            perp_bail!(ErrorId::Config, ErrorDomain::Market, "trading_fee_counter_collateral must be in the range 0 to 0.0999 inclusive ({} is invalid)", self.trading_fee_counter_collateral );
        }

        if self.crank_execs == 0 {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "crank_execs_per_batch must be greater than zero"
            );
        }

        if self.max_leverage <= Number::ONE {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "max_leverage must be greater than one ({} is invalid)",
                self.max_leverage
            );
        }

        if self.carry_leverage <= Decimal256::one() {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "carry_leverage must be greater than one ({} is invalid)",
                self.carry_leverage
            );
        }

        if (self.carry_leverage.into_number() + Number::ONE)? > self.max_leverage {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "carry_leverage must be at least one less than max_leverage ({} is invalid, max_leverage is {})",
                self.carry_leverage,
                self.max_leverage
            );
        }

        if self.borrow_fee_rate_max_annualized < self.borrow_fee_rate_min_annualized {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "borrow_fee_rate_min_annualized ({}) must be less than borrow_fee_rate_max_annualized ({})",
                self.borrow_fee_rate_min_annualized,
                self.borrow_fee_rate_max_annualized
            );
        }

        if self.protocol_tax >= Decimal256::one() {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "protocol_tax must be less than or equal to 1 ({} is invalid)",
                self.protocol_tax
            );
        }

        if self.unstake_period_seconds == 0 {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "unstake period must be greater than 0 ({} is invalid)",
                self.unstake_period_seconds
            );
        }

        if Number::from(self.target_utilization) >= Number::ONE {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "Target utilization ratio must be between 0 and 1 exclusive ({} is invalid)",
                self.target_utilization
            );
        }

        if Number::from(self.min_xlp_rewards_multiplier) < Number::ONE {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "Min xLP rewards multiplier must be at least 1 ({} is invalid)",
                self.max_xlp_rewards_multiplier
            )
        }

        if self.max_xlp_rewards_multiplier < self.min_xlp_rewards_multiplier {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "Max xLP rewards multiplier ({}) must be greater than or equal to the min ({})",
                self.max_xlp_rewards_multiplier,
                self.min_xlp_rewards_multiplier
            )
        }

        if self.crank_fee_charged < self.crank_fee_reward {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "Crank fee charged ({}) must be greater than or equal to the crank fee reward ({})",
                self.crank_fee_charged,
                self.crank_fee_reward
            )
        }

        if self.delta_neutrality_fee_tax > Decimal256::one() {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "Delta neutrality fee tax ({}) must be less than or equal to 1",
                self.delta_neutrality_fee_tax
            )
        }

        if self.liquifunding_delay_fuzz_seconds >= self.liquifunding_delay_seconds {
            perp_bail!(
                ErrorId::Config,
                ErrorDomain::Market,
                "Liquifunding delay fuzz ({}) must be less than or equal to the liquifunding delay ({})",
                self.liquifunding_delay_fuzz_seconds,
                self.liquifunding_delay_seconds,
            )
        }

        Ok(())
    }
}

/// Helper struct to conveniently update [Config]
///
/// For each field below, please see the corresponding [Config] field's
/// documentation.
#[cw_serde]
#[allow(missing_docs)]
#[derive(Default)]
pub struct ConfigUpdate {
    pub trading_fee_notional_size: Option<Decimal256>,
    pub trading_fee_counter_collateral: Option<Decimal256>,
    pub crank_execs: Option<u32>,
    pub max_leverage: Option<Number>,
    pub carry_leverage: Option<Decimal256>,
    pub funding_rate_sensitivity: Option<Decimal256>,
    pub funding_rate_max_annualized: Option<Decimal256>,
    pub borrow_fee_rate_min_annualized: Option<NumberGtZero>,
    pub borrow_fee_rate_max_annualized: Option<NumberGtZero>,
    pub mute_events: Option<bool>,
    pub liquifunding_delay_seconds: Option<u32>,
    pub protocol_tax: Option<Decimal256>,
    pub unstake_period_seconds: Option<u32>,
    pub target_utilization: Option<NumberGtZero>,
    pub borrow_fee_sensitivity: Option<NumberGtZero>,
    pub max_xlp_rewards_multiplier: Option<NumberGtZero>,
    pub min_xlp_rewards_multiplier: Option<NumberGtZero>,
    pub delta_neutrality_fee_sensitivity: Option<NumberGtZero>,
    pub delta_neutrality_fee_cap: Option<NumberGtZero>,
    pub delta_neutrality_fee_tax: Option<Decimal256>,
    pub crank_fee_charged: Option<Usd>,
    pub crank_fee_surcharge: Option<Usd>,
    pub crank_fee_reward: Option<Usd>,
    pub minimum_deposit_usd: Option<Usd>,
    pub liquifunding_delay_fuzz_seconds: Option<u32>,
    pub max_liquidity: Option<MaxLiquidity>,
    pub disable_position_nft_exec: Option<bool>,
    pub liquidity_cooldown_seconds: Option<u32>,
    pub spot_price: Option<SpotPriceConfigInit>,
    pub exposure_margin_ratio: Option<Decimal256>,
    pub referral_reward_ratio: Option<Decimal256>,
}
#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for ConfigUpdate {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            trading_fee_notional_size: arbitrary_decimal_256_option(u)?,
            trading_fee_counter_collateral: arbitrary_decimal_256_option(u)?,
            crank_execs: u.arbitrary()?,
            max_leverage: u.arbitrary()?,
            carry_leverage: arbitrary_decimal_256_option(u)?,
            funding_rate_sensitivity: arbitrary_decimal_256_option(u)?,
            funding_rate_max_annualized: arbitrary_decimal_256_option(u)?,
            borrow_fee_rate_min_annualized: u.arbitrary()?,
            borrow_fee_rate_max_annualized: u.arbitrary()?,
            mute_events: u.arbitrary()?,
            liquifunding_delay_seconds: u.arbitrary()?,
            protocol_tax: arbitrary_decimal_256_option(u)?,
            unstake_period_seconds: u.arbitrary()?,
            target_utilization: u.arbitrary()?,
            borrow_fee_sensitivity: u.arbitrary()?,
            max_xlp_rewards_multiplier: u.arbitrary()?,
            min_xlp_rewards_multiplier: u.arbitrary()?,
            delta_neutrality_fee_sensitivity: u.arbitrary()?,
            delta_neutrality_fee_cap: u.arbitrary()?,
            delta_neutrality_fee_tax: arbitrary_decimal_256_option(u)?,
            crank_fee_charged: u.arbitrary()?,
            crank_fee_surcharge: u.arbitrary()?,
            crank_fee_reward: u.arbitrary()?,
            minimum_deposit_usd: u.arbitrary()?,
            liquifunding_delay_fuzz_seconds: None,
            max_liquidity: None,
            disable_position_nft_exec: None,
            liquidity_cooldown_seconds: None,
            exposure_margin_ratio: arbitrary_decimal_256_option(u)?,
            referral_reward_ratio: None,
            spot_price: None,
        })
    }
}

impl From<Config> for ConfigUpdate {
    fn from(src: Config) -> Self {
        Self {
            trading_fee_notional_size: Some(src.trading_fee_notional_size),
            trading_fee_counter_collateral: Some(src.trading_fee_counter_collateral),
            crank_execs: Some(src.crank_execs),
            max_leverage: Some(src.max_leverage),
            carry_leverage: Some(src.carry_leverage),
            funding_rate_sensitivity: Some(src.funding_rate_sensitivity),
            funding_rate_max_annualized: Some(src.funding_rate_max_annualized),
            mute_events: Some(src.mute_events),
            liquifunding_delay_seconds: Some(src.liquifunding_delay_seconds),
            protocol_tax: Some(src.protocol_tax),
            unstake_period_seconds: Some(src.unstake_period_seconds),
            target_utilization: Some(src.target_utilization),
            borrow_fee_sensitivity: Some(src.borrow_fee_sensitivity),
            borrow_fee_rate_min_annualized: Some(src.borrow_fee_rate_min_annualized),
            borrow_fee_rate_max_annualized: Some(src.borrow_fee_rate_max_annualized),
            max_xlp_rewards_multiplier: Some(src.max_xlp_rewards_multiplier),
            min_xlp_rewards_multiplier: Some(src.min_xlp_rewards_multiplier),
            delta_neutrality_fee_sensitivity: Some(src.delta_neutrality_fee_sensitivity),
            delta_neutrality_fee_cap: Some(src.delta_neutrality_fee_cap),
            delta_neutrality_fee_tax: Some(src.delta_neutrality_fee_tax),
            crank_fee_charged: Some(src.crank_fee_charged),
            crank_fee_surcharge: Some(src.crank_fee_surcharge),
            crank_fee_reward: Some(src.crank_fee_reward),
            minimum_deposit_usd: Some(src.minimum_deposit_usd),
            liquifunding_delay_fuzz_seconds: Some(src.liquifunding_delay_fuzz_seconds),
            max_liquidity: Some(src.max_liquidity),
            disable_position_nft_exec: Some(src.disable_position_nft_exec),
            liquidity_cooldown_seconds: Some(src.liquidity_cooldown_seconds),
            exposure_margin_ratio: Some(src.exposure_margin_ratio),
            referral_reward_ratio: Some(src.referral_reward_ratio),
            spot_price: Some(src.spot_price.into()),
        }
    }
}
