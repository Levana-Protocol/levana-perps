//! We're going to model a stablecoin-denominated pair. Mathematically it's the
//! same as crypto-denominated pairs, but the results will be easier to
//! understand. We're also less likely to make mistakes in the implementation.
//!
//! We ignore funding fee and borrow fee payments, as they scale with the
//! duration of the open position, and an attacker will try to close positions
//! quickly.
//!
//! We assume infinite liquidity available in the pool.
//!
//! We do not specifically look for liquidations. Instead, when closing a
//! position, an attacker simply loses all their collateral.
//!
//! For calculating artificial slippage, we assume that the protocol was
//! delta-neutral between longs and shorts before the attacker placed his
//! position. (As we develop this further, need to decide the rate of arbitrage
//! for multi-step attacks).

use anyhow::{ensure, Result};

use crate::{
    config::{Config, SlippageRules},
    types::{Asset, Direction, Price, Usd, Wallet},
};

#[derive(Debug)]
pub(crate) struct Position {
    pub(crate) notional: Asset,
    pub(crate) trader_collateral: Usd,
    /// Counter side collateral
    pub(crate) cs_collateral: Usd,
    pub(crate) entry_price: Price,
}

#[derive(Debug)]
pub(crate) struct Perps {
    pub(crate) liquidity_pool: Usd,
    pub(crate) trading_fees: Usd,
    pub(crate) artificial_slippage: Usd,
    /// Balance of positions in the protocol (longs versus shorts, shorts being negative)
    pub(crate) notional: Asset,
}

impl Perps {
    pub(crate) fn new() -> Self {
        Perps {
            liquidity_pool: Usd(0.0),
            trading_fees: Usd(0.0),
            artificial_slippage: Usd(0.0),
            notional: Asset(0.0),
        }
    }

    /// Performs sanity checks that all provided values are valid.
    pub(crate) fn open(
        &mut self,
        position @ Position {
            notional,
            trader_collateral,
            cs_collateral,
            entry_price,
        }: Position,
        config: &Config,
    ) -> Result<(Wallet, Position)> {
        ensure!(trader_collateral.0 > 0.0);
        ensure!(cs_collateral.0 > 0.0);
        ensure!(notional.0 != 0.0);

        // Trader side leverage check
        config.check_max_leverage((notional * entry_price / trader_collateral).abs())?;
        // Counter side leverage check
        config.check_max_leverage((notional * entry_price / cs_collateral).abs())?;

        // By our assumptions above, we always start in a balanced position for
        // the protocol. Therefore, the artificial slippage cap cannot yet have
        // been reached. If we later add multi-step attacks with balancing
        // arbitrage, we need to track the notional delta and perform the check
        // from section 3.1.2 instead.

        // Since we assume infinite liquidity, no need to check open interest balance

        let trading_fees = notional.abs() * entry_price * config.trading_fee_rate
            + cs_collateral * config.cs_trading_fee_rate;
        let artificial_slippage = self.artificial_slippage(config, notional, entry_price, true)?;

        self.artificial_slippage += artificial_slippage;
        self.liquidity_pool -= cs_collateral;
        self.trading_fees += trading_fees;
        self.notional += notional;

        let wallet = Wallet {
            usd: -(trader_collateral + trading_fees + artificial_slippage),
            asset: Asset(0.0),
        };

        log::debug!("Opening position: {wallet:?} {position:?}. Slippage: {artificial_slippage:?}");

        Ok((wallet, position))
    }

    /// Calculate the artificial slippage (positive == from trader, negative == to trader) from the given movement
    ///
    /// If `is_open` is `true`, ensures that this position does not enter us
    /// into capping or push us further into capping.
    fn artificial_slippage(
        &self,
        config: &Config,
        delta_notional: Asset,
        current_price: Price,
        is_open: bool,
    ) -> Result<Usd> {
        if let SlippageRules::NoSlippage = config.slippage {
            // optimization, exit early
            return Ok(Usd(0.0));
        }

        let is_capped_low = |x| x <= -config.slippage_cap;
        let is_capped_high = |x| x >= config.slippage_cap;

        let instant_artificial_slippage_before_uncapped = self.notional / config.slippage_k;
        let instant_artificial_slippage_after_uncapped =
            (self.notional + delta_notional) / config.slippage_k;

        let is_capped_low_before = is_capped_low(instant_artificial_slippage_before_uncapped);
        let is_capped_high_before = is_capped_high(instant_artificial_slippage_before_uncapped);
        let is_capped_low_after = is_capped_low(instant_artificial_slippage_after_uncapped);
        let is_capped_high_after = is_capped_high(instant_artificial_slippage_after_uncapped);

        if is_open {
            if is_capped_low_before {
                // We were already too short, disallow more shorts
                anyhow::ensure!(delta_notional.0 > 0.0, "Cannot open short while capped low");

                // We don't allow the user to swing the market all the way from capped low to capped high
                anyhow::ensure!(
                    !is_capped_high_after,
                    "Cannot open large enough position to move from capped low to capped high"
                );
            } else if is_capped_high_before {
                // Same conditions as above but inversed
                anyhow::ensure!(delta_notional.0 < 0.0, "Cannot open long while capped high");
                anyhow::ensure!(
                    !is_capped_low_after,
                    "Cannot open large enough position to move from capped high to capped low"
                );
            } else if is_capped_low_after {
                assert!(delta_notional.0 < 0.0);
                anyhow::bail!(
                    "Cannot open a short position to enter a low capped slippage position"
                );
            } else if is_capped_high_after {
                assert!(delta_notional.0 > 0.0);
                anyhow::bail!(
                    "Cannot open a long position to enter a high capped slippage position"
                );
            }
        }

        let notional_low_cap = -config.slippage_cap * config.slippage_k.0;
        let notional_high_cap = config.slippage_cap * config.slippage_k.0;

        let delta_notional_at_low_cap = (self.notional.0 + delta_notional.0).min(notional_low_cap)
            - self.notional.0.min(notional_low_cap);
        let delta_notional_at_high_cap = (self.notional.0 + delta_notional.0)
            .max(notional_high_cap)
            - self.notional.0.max(notional_high_cap);
        let delta_notional_uncapped =
            delta_notional.0 - delta_notional_at_low_cap - delta_notional_at_high_cap;

        let mut artificial_slippage_amount = delta_notional_at_low_cap * (-config.slippage_cap);
        artificial_slippage_amount += delta_notional_at_high_cap * config.slippage_cap;
        artificial_slippage_amount += (delta_notional_uncapped * delta_notional_uncapped
            + 2.0 * delta_notional_uncapped * self.notional.0)
            / (2.0 * config.slippage_k.0);

        let both_capped = (is_capped_low_before || is_capped_high_before)
            && (is_capped_low_after || is_capped_high_after);

        log::debug!(
            "Instant before {} after {} cap {} both capped? {both_capped}",
            instant_artificial_slippage_before_uncapped,
            instant_artificial_slippage_after_uncapped,
            config.slippage_cap
        );
        let artificial_slippage_usd = Asset(artificial_slippage_amount) * current_price;
        let to_trader = artificial_slippage_usd.0 < 0.0;

        Ok(match config.slippage {
            SlippageRules::NoSlippage => panic!("This should have been caught above!"),
            SlippageRules::TraderFullSlippage => artificial_slippage_usd,
            SlippageRules::TraderHalfSlippage if to_trader => artificial_slippage_usd / 2.0,
            SlippageRules::TraderHalfSlippage => artificial_slippage_usd,
            SlippageRules::UnidirectionalSlippage if to_trader => Usd(0.0),
            SlippageRules::UnidirectionalSlippage => artificial_slippage_usd,
        })
    }

    pub(crate) fn close(
        &mut self,
        Position {
            notional,
            trader_collateral,
            cs_collateral,
            entry_price,
        }: Position,
        exit_price: Price,
        config: &Config,
    ) -> Wallet {
        let slippage = self
            .artificial_slippage(config, -notional, exit_price, false)
            .expect("Closing a position cannot be blocked by artificial slippage");

        // Assume no funding payment for borrow fee payment

        let price_delta = Price(exit_price.0 - entry_price.0);

        let total_pot = trader_collateral + cs_collateral;

        let trader_raw_profit = notional * price_delta;

        let usd = if -trader_raw_profit > trader_collateral {
            // Liquidation
            log::debug!("Liquidation!");
            self.liquidity_pool += total_pot;
            Usd(0.0)
        } else if trader_raw_profit > cs_collateral {
            // Max profit
            log::debug!("Max profit!");
            total_pot
        } else {
            // Divvy it up
            self.liquidity_pool += cs_collateral - trader_raw_profit;
            trader_collateral + trader_raw_profit
        };

        self.artificial_slippage += slippage;
        self.notional = Asset(self.notional.0 - notional.0);
        let usd = usd - slippage;

        log::debug!("Closing a position, receiving {usd:?}. Price delta: {price_delta:?}. Slippage: {slippage:?}");

        Wallet {
            usd,
            asset: Asset(0.0),
        }
    }
}

impl Position {
    pub(crate) fn direction(&self) -> Direction {
        if self.notional.0 > 0.0 {
            Direction::Long
        } else {
            Direction::Short
        }
    }
}
