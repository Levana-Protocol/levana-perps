//! Data types for tracking liquidity
use anyhow::{Context, Result};
use cosmwasm_schema::cw_serde;
use shared::prelude::*;

/// Protocol wide stats on liquidity
#[cw_serde]
#[derive(Default)]
pub struct LiquidityStats {
    /// Collateral locked as counter collateral in the protocol
    pub locked: Collateral,
    /// Total amount of collateral available to be used as liquidity
    pub unlocked: Collateral,
    /// Total number of LP tokens
    pub total_lp: LpToken,
    /// Total number of xLP tokens
    pub total_xlp: LpToken,
}

impl LiquidityStats {
    /// Total amount of locked and unlocked collateral.
    pub fn total_collateral(&self) -> Collateral {
        self.locked + self.unlocked
    }

    /// Total number of LP and xLP tokens
    pub fn total_tokens(&self) -> LpToken {
        self.total_lp + self.total_xlp
    }

    /// Calculate the amount of collateral for a given number of LP tokens
    ///
    /// This method can fail due to arithmetic overflow. It can also fail if
    /// invariants are violated, specifically if there is 0 collateral in the
    /// pool when this is called with a non-zero amount of LP.
    ///
    /// Note that even with a non-zero input value for `lp`, due to rounding
    /// errors this function may return 0 collateral.
    pub fn lp_to_collateral(&self, lp: LpToken) -> Result<Collateral> {
        if lp.is_zero() {
            return Ok(Collateral::zero());
        }
        let total_collateral = self.total_collateral();

        anyhow::ensure!(
            !total_collateral.approx_eq(Collateral::zero()),
            "LiquidityStats::lp_to_collateral: no liquidity is in the pool"
        );
        let total_tokens = self.total_tokens();
        debug_assert_ne!(total_tokens, LpToken::zero());

        Ok(Collateral::from_decimal256(
            total_collateral
                .into_decimal256()
                .checked_mul(lp.into_decimal256())?
                .checked_div(total_tokens.into_decimal256())?,
        ))
    }

    /// Same as [Self::lp_to_collateral], but treats round-to-zero as an error.
    pub fn lp_to_collateral_non_zero(&self, lp: NonZero<LpToken>) -> Result<NonZero<Collateral>> {
        self.lp_to_collateral(lp.raw()).and_then(|c| {
            NonZero::new(c)
                .context("lp_to_collateral_non_zero: amount of backing collateral rounded to 0")
        })
    }

    /// Calculate how many LP tokens would be produced from the given collateral.
    ///
    /// If there is currently no liquidity in the pool, this will use a 1:1 ratio.
    pub fn collateral_to_lp(&self, amount: NonZero<Collateral>) -> Result<NonZero<LpToken>> {
        let total_collateral = self.total_collateral();

        NonZero::new(LpToken::from_decimal256(if total_collateral.is_zero() {
            debug_assert!(self.total_lp.is_zero());
            debug_assert!(self.total_xlp.is_zero());
            amount.into_decimal256()
        } else {
            self.total_tokens()
                .into_decimal256()
                .checked_mul(amount.into_decimal256())?
                .checked_div(total_collateral.into_decimal256())?
        }))
        .context("liquidity_deposit_inner: new shares is (impossibly) 0")
    }
}

/// Liquidity events
pub mod events {
    use super::LiquidityStats;
    use cosmwasm_std::Event;
    use shared::prelude::*;

    /// Liquidity was withdrawn from the system
    pub struct WithdrawEvent {
        /// Number of LP tokens burned
        pub burned_shares: NonZero<LpToken>,
        /// Collateral returned to the provider
        pub withdrawn_funds: NonZero<Collateral>,
        /// USD value of the collateral
        pub withdrawn_funds_usd: NonZero<Usd>,
    }

    impl PerpEvent for WithdrawEvent {}
    impl From<WithdrawEvent> for cosmwasm_std::Event {
        fn from(src: WithdrawEvent) -> Self {
            cosmwasm_std::Event::new("liquidity-withdraw").add_attributes(vec![
                ("burned-shares", src.burned_shares.to_string()),
                ("withdrawn-funds", src.withdrawn_funds.to_string()),
                ("withdrawn-funds-usd", src.withdrawn_funds_usd.to_string()),
            ])
        }
    }

    /// Liquidity deposited into the protocol
    pub struct DepositEvent {
        /// Amount of collateral deposited
        pub amount: NonZero<Collateral>,
        /// Value of deposit in USD
        pub amount_usd: NonZero<Usd>,
        /// Number of tokens minted from this deposit
        pub shares: NonZero<LpToken>,
    }

    impl PerpEvent for DepositEvent {}
    impl From<DepositEvent> for cosmwasm_std::Event {
        fn from(src: DepositEvent) -> Self {
            cosmwasm_std::Event::new("liquidity-deposit").add_attributes(vec![
                ("amount", src.amount.to_string()),
                ("amount-usd", src.amount_usd.to_string()),
                ("shares", src.shares.to_string()),
            ])
        }
    }

    /// Liquidity was locked into a position as counter collateral.
    pub struct LockEvent {
        /// Amount of liquidity that was locked
        pub amount: NonZero<Collateral>,
    }

    impl PerpEvent for LockEvent {}
    impl From<LockEvent> for cosmwasm_std::Event {
        fn from(src: LockEvent) -> Self {
            cosmwasm_std::Event::new("liquidity-lock")
                .add_attribute("amount", src.amount.to_string())
        }
    }

    /// Liquidity was unlocked from a position back into the unlocked pool.
    pub struct UnlockEvent {
        /// Amount of liquidity that was unlocked.
        pub amount: NonZero<Collateral>,
    }

    impl PerpEvent for UnlockEvent {}
    impl From<UnlockEvent> for cosmwasm_std::Event {
        fn from(src: UnlockEvent) -> Self {
            cosmwasm_std::Event::new("liquidity-unlock")
                .add_attribute("amount", src.amount.to_string())
        }
    }

    /// Amount of locked liquidity changed from price exposure in liquifunding.
    pub struct LockUpdateEvent {
        /// Increase or decrease in locked liquidity in the pool.
        pub amount: Signed<Collateral>,
    }

    impl PerpEvent for LockUpdateEvent {}
    impl From<LockUpdateEvent> for cosmwasm_std::Event {
        fn from(src: LockUpdateEvent) -> Self {
            cosmwasm_std::Event::new("liquidity-update")
                .add_attributes(vec![("amount", src.amount.to_string())])
        }
    }

    /// Provides current size of the liquidity pool
    pub struct LiquidityPoolSizeEvent {
        /// Total locked collateral
        pub locked: Collateral,
        /// Locked collateral in USD
        pub locked_usd: Usd,
        /// Total unlocked collateral
        pub unlocked: Collateral,
        /// Unlocked collateral in USD
        pub unlocked_usd: Usd,
        /// Total collateral (locked and unlocked) backing LP tokens
        pub lp_collateral: Collateral,
        /// Total collateral (locked and unlocked) backing xLP tokens
        pub xlp_collateral: Collateral,
        /// Total number of LP tokens
        pub total_lp: LpToken,
        /// Total number of xLP tokens
        pub total_xlp: LpToken,
    }

    impl LiquidityPoolSizeEvent {
        /// Generate a value from protocol stats and the current price.
        pub fn from_stats(stats: &LiquidityStats, price: &PricePoint) -> Self {
            let total_collateral = stats.total_collateral();
            let total_tokens = stats.total_tokens();
            let (lp_collateral, xlp_collateral) = if total_tokens.is_zero() {
                debug_assert_eq!(total_collateral, Collateral::zero());
                (Collateral::zero(), Collateral::zero())
            } else {
                let lp_collateral = total_collateral.into_decimal256()
                    * stats.total_lp.into_decimal256()
                    / total_tokens.into_decimal256();
                let lp_collateral = Collateral::from_decimal256(lp_collateral);
                let xlp_collateral = total_collateral - lp_collateral;
                (lp_collateral, xlp_collateral)
            };
            Self {
                locked: stats.locked,
                locked_usd: price.collateral_to_usd(stats.locked),
                unlocked: stats.unlocked,
                unlocked_usd: price.collateral_to_usd(stats.unlocked),
                lp_collateral,
                xlp_collateral,
                total_lp: stats.total_lp,
                total_xlp: stats.total_xlp,
            }
        }
    }

    impl PerpEvent for LiquidityPoolSizeEvent {}

    impl From<LiquidityPoolSizeEvent> for cosmwasm_std::Event {
        fn from(src: LiquidityPoolSizeEvent) -> Self {
            cosmwasm_std::Event::new("liquidity-pool-size").add_attributes(vec![
                ("locked", src.locked.to_string()),
                ("locked-usd", src.locked_usd.to_string()),
                ("unlocked", src.unlocked.to_string()),
                ("unlocked-usd", src.unlocked_usd.to_string()),
                ("lp-collateral", src.lp_collateral.to_string()),
                ("xlp-collateral", src.xlp_collateral.to_string()),
                ("total-lp", src.total_lp.to_string()),
                ("total-xlp", src.total_xlp.to_string()),
            ])
        }
    }

    impl TryFrom<Event> for LiquidityPoolSizeEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(LiquidityPoolSizeEvent {
                locked: evt.decimal_attr("locked")?,
                locked_usd: evt.decimal_attr("locked-usd")?,
                unlocked: evt.decimal_attr("unlocked")?,
                unlocked_usd: evt.decimal_attr("unlocked-usd")?,
                lp_collateral: evt.decimal_attr("lp-collateral")?,
                xlp_collateral: evt.decimal_attr("xlp-collateral")?,
                total_lp: evt.decimal_attr("total-lp")?,
                total_xlp: evt.decimal_attr("total-xlp")?,
            })
        }
    }

    /// Tracks when the delta neutrality ratio has been updated.
    #[derive(Debug)]
    pub struct DeltaNeutralityRatioEvent {
        /// Total locked and unlocked liquidity in the pool
        pub total_liquidity: Collateral,
        /// Total long interest (using direction to base), in notional
        pub long_interest: Notional,
        /// Total short interest (using direction to base), in notional
        pub short_interest: Notional,
        /// Net notional: long - short
        pub net_notional: Signed<Notional>,
        /// Current notional price
        pub price_notional: Price,
        /// Current delta neutrality ratio: net-notional in collateral / total liquidity.
        pub delta_neutrality_ratio: Signed<Decimal256>,
    }

    impl PerpEvent for DeltaNeutralityRatioEvent {}

    impl From<DeltaNeutralityRatioEvent> for cosmwasm_std::Event {
        fn from(
            DeltaNeutralityRatioEvent {
                total_liquidity,
                long_interest,
                short_interest,
                net_notional,
                price_notional,
                delta_neutrality_ratio,
            }: DeltaNeutralityRatioEvent,
        ) -> Self {
            cosmwasm_std::Event::new("delta-neutrality-ratio").add_attributes(vec![
                ("total-liquidity", total_liquidity.to_string()),
                ("long-interest", long_interest.to_string()),
                ("short-interest", short_interest.to_string()),
                ("net-notional", net_notional.to_string()),
                ("price-notional", price_notional.to_string()),
                ("delta-neutrality-ratio", delta_neutrality_ratio.to_string()),
            ])
        }
    }

    impl TryFrom<Event> for DeltaNeutralityRatioEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(DeltaNeutralityRatioEvent {
                total_liquidity: evt.decimal_attr("total-liquidity")?,
                long_interest: evt.decimal_attr("long-interest")?,
                short_interest: evt.decimal_attr("short-interest")?,
                net_notional: evt.number_attr("net-notional")?,
                price_notional: Price::try_from_number(evt.number_attr("price-notional")?)?,
                delta_neutrality_ratio: evt.number_attr("delta-neutrality-ratio")?,
            })
        }
    }
}
