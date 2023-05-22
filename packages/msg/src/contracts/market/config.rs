//! Market-wide configuration
mod defaults;
use shared::prelude::*;

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
    /// How old must the latest price update be to trigger the protocol to lock?
    pub price_update_too_old_seconds: u32,
    /// How far behind must the position liquifunding process be to consider the protocol stale?
    pub staleness_seconds: u32,
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
    pub delta_neutrality_fee_sensitivity: NumberGtZero,
    /// Delta neutrality fee cap parameter, given as a percentage
    pub delta_neutrality_fee_cap: NumberGtZero,
    /// Proportion of delta neutrality inflows that are sent to the protocol.
    pub delta_neutrality_fee_tax: Decimal256,
    /// The fee to set a [super::entry::ExecuteMsg::PlaceLimitOrder]
    pub limit_order_fee: Collateral,
    /// The crank fee to be paid into the system, in collateral
    pub crank_fee_charged: Usd,
    /// The crank fee to be sent to crankers, in collateral
    pub crank_fee_reward: Usd,
    /// Minimum deposit collateral, given in USD
    pub minimum_deposit_usd: Usd,
    /// How many positions can sit in "unpend" before we disable new open/update positions for congestion.
    #[serde(default = "defaults::unpend_limit")]
    pub unpend_limit: u32,
    /// The liquifunding delay fuzz factor, in seconds.
    ///
    /// Up to how many seconds will we perform a liquifunding early. This will
    /// be part of a semi-randomly generated value and will allow us to schedule
    /// liquifundings arbitrarily to smooth out spikes in traffic.
    #[serde(default = "defaults::liquifunding_delay_fuzz_seconds")]
    pub liquifunding_delay_fuzz_seconds: u32,
    /// The maximum amount of liquidity that can be deposited into the market.
    pub max_liquidity: MaxLiquidity,
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

impl Default for Config {
    fn default() -> Self {
        // these unwraps are fine since we define the value
        Self {
            trading_fee_notional_size: "0.0005".parse().unwrap(),
            trading_fee_counter_collateral: "0.0005".parse().unwrap(),
            crank_execs: 7,
            max_leverage: Number::try_from("30").unwrap(),
            carry_leverage: "29".parse().unwrap(),
            funding_rate_max_annualized: "0.9".parse().unwrap(),
            borrow_fee_rate_min_annualized: "0.01".parse().unwrap(),
            borrow_fee_rate_max_annualized: "0.60".parse().unwrap(),
            funding_rate_sensitivity: "1".parse().unwrap(),
            mute_events: false,
            liquifunding_delay_seconds: 60 * 60 * 24,
            price_update_too_old_seconds: 60 * 30,
            staleness_seconds: 60 * 60 * 2,
            protocol_tax: "0.3".parse().unwrap(),
            unstake_period_seconds: 60 * 60 * 24 * 21, // 21 days
            target_utilization: "0.9".parse().unwrap(),
            // Try to realize the bias over a 3 day period.
            //
            // See: https://phobosfinance.atlassian.net/browse/PERP-606
            borrow_fee_sensitivity: (Number::ONE / Number::try_from("3").unwrap())
                .try_into()
                .unwrap(),
            max_xlp_rewards_multiplier: "2".parse().unwrap(),
            min_xlp_rewards_multiplier: "1".parse().unwrap(),
            delta_neutrality_fee_sensitivity: "50000000".parse().unwrap(),
            delta_neutrality_fee_cap: "0.01".parse().unwrap(),
            delta_neutrality_fee_tax: "0.25".parse().unwrap(),
            limit_order_fee: Collateral::from(0u64),
            crank_fee_charged: "0.01".parse().unwrap(),
            crank_fee_reward: "0.001".parse().unwrap(),
            minimum_deposit_usd: "5".parse().unwrap(),
            unpend_limit: 50,
            liquifunding_delay_fuzz_seconds: 60 * 60 * 4,
            max_liquidity: MaxLiquidity::Unlimited {},
        }
    }
}

impl Config {
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

        if self.carry_leverage.into_number() + Number::ONE > self.max_leverage {
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

    /// How long between calculation of a liquidation margin for a position and
    /// the protocol going stale?
    ///
    /// When calculating liquidation margin, we set aside enough funds to cover
    /// the liquifunding delay, plus a staleness buffer. This method returns the
    /// sum of those two numbers. Once that amount of time has passed, and the
    /// position has not been liquifunded or closed, the protocol is in a stale
    /// state because we cannot guarantee liquidity of the position.
    pub(crate) fn liquidation_margin_duration(&self) -> Duration {
        Duration::from_seconds(
            u64::from(self.liquifunding_delay_seconds) + u64::from(self.staleness_seconds),
        )
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
    pub price_update_too_old_seconds: Option<u32>,
    pub staleness_seconds: Option<u32>,
    pub protocol_tax: Option<Decimal256>,
    pub unstake_period_seconds: Option<u32>,
    pub target_utilization: Option<NumberGtZero>,
    pub borrow_fee_sensitivity: Option<NumberGtZero>,
    pub max_xlp_rewards_multiplier: Option<NumberGtZero>,
    pub min_xlp_rewards_multiplier: Option<NumberGtZero>,
    pub delta_neutrality_fee_sensitivity: Option<NumberGtZero>,
    pub delta_neutrality_fee_cap: Option<NumberGtZero>,
    pub delta_neutrality_fee_tax: Option<Decimal256>,
    pub limit_order_fee: Option<Collateral>,
    pub crank_fee_charged: Option<Usd>,
    pub crank_fee_reward: Option<Usd>,
    pub minimum_deposit_usd: Option<Usd>,
    pub unpend_limit: Option<u32>,
    pub liquifunding_delay_fuzz_seconds: Option<u32>,
    pub max_liquidity: Option<MaxLiquidity>,
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
            price_update_too_old_seconds: u.arbitrary()?,
            staleness_seconds: u.arbitrary()?,
            protocol_tax: arbitrary_decimal_256_option(u)?,
            unstake_period_seconds: u.arbitrary()?,
            target_utilization: u.arbitrary()?,
            borrow_fee_sensitivity: u.arbitrary()?,
            max_xlp_rewards_multiplier: u.arbitrary()?,
            min_xlp_rewards_multiplier: u.arbitrary()?,
            delta_neutrality_fee_sensitivity: u.arbitrary()?,
            delta_neutrality_fee_cap: u.arbitrary()?,
            delta_neutrality_fee_tax: arbitrary_decimal_256_option(u)?,
            limit_order_fee: u.arbitrary()?,
            crank_fee_charged: u.arbitrary()?,
            crank_fee_reward: u.arbitrary()?,
            minimum_deposit_usd: u.arbitrary()?,
            unpend_limit: None,
            liquifunding_delay_fuzz_seconds: None,
            max_liquidity: None,
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
            price_update_too_old_seconds: Some(src.price_update_too_old_seconds),
            staleness_seconds: Some(src.staleness_seconds),
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
            limit_order_fee: Some(src.limit_order_fee),
            crank_fee_charged: Some(src.crank_fee_charged),
            crank_fee_reward: Some(src.crank_fee_reward),
            minimum_deposit_usd: Some(src.minimum_deposit_usd),
            unpend_limit: Some(src.unpend_limit),
            liquifunding_delay_fuzz_seconds: Some(src.liquifunding_delay_fuzz_seconds),
            max_liquidity: Some(src.max_liquidity),
        }
    }
}
