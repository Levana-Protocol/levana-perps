use anyhow::Result;

use crate::{
    config::{Config, SlippageRules},
    system::System,
    types::Usd,
};

/// Represent attacks as an enum to allow for future variation in the attack
/// vector.
#[derive(Debug)]
pub(crate) enum Attack {
    /// Standard four-stage long attack we've analyzed for price manipulation: open long, buy spot, close long, sell spot.
    StandardLong {
        /// How much USD is used to open the long position
        long_size: Usd,
        /// What leverage is used to open the long position
        leverage: f64,
        /// How much USD is used to move the spot price
        buy_size: Usd,
    },
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct Simulation {
    pub(crate) attack_type: &'static str,
    pub(crate) attack_description: String,
    pub(crate) attacker_collateral: Usd,
    pub(crate) attacker_leverage: f64,
    pub(crate) attacker_profits: Usd,
    pub(crate) artificial_slippage: Usd,
    pub(crate) trading_fees: Usd,
    pub(crate) attacker_roi: f64,
    pub(crate) arbitrage_rate: Usd,
    pub(crate) twap_blocks: usize,
    pub(crate) house_wins: bool,
    pub(crate) max_leverage: f64,
    pub(crate) trading_fee_rate: f64,
    pub(crate) cs_trading_fee_rate: f64,
    pub(crate) slippage_cap: f64,
    pub(crate) slippage: SlippageRules,
}

impl Attack {
    pub(crate) fn simulate(&self, config: &Config) -> Result<Simulation> {
        let Attack::StandardLong {
            long_size,
            leverage,
            buy_size,
        } = self;

        let mut system = System::new(config);
        self.helper(&mut system)?;

        system.close_arbitragers();
        let attacker_collateral = *long_size + *buy_size;
        let (attacker_profits, attacker_roi) = system.summary(attacker_collateral);

        system.check_coherence().unwrap();

        Ok(Simulation {
            attack_type: "standard-long",
            attack_description: format!("{self:?}"),
            attacker_collateral,
            attacker_leverage: *leverage,
            attacker_profits,
            attacker_roi,
            arbitrage_rate: config.arbitrage_rate,
            twap_blocks: config.twap_blocks,
            house_wins: config.house_wins,
            max_leverage: config.max_leverage,
            trading_fee_rate: config.trading_fee_rate,
            cs_trading_fee_rate: config.cs_trading_fee_rate,
            slippage_cap: config.slippage_cap,
            artificial_slippage: system.perps.artificial_slippage,
            trading_fees: system.perps.trading_fees,
            slippage: config.slippage,
        })
    }

    fn helper(&self, system: &mut System) -> Result<()> {
        let Attack::StandardLong {
            long_size,
            leverage,
            buy_size,
        } = self;

        for _ in 0..10 {
            system.step();
        }

        system.attack_start_perps_up(*long_size, *leverage)?;
        system.step();
        system.attack_start_spot_up(*buy_size);

        for _ in 0..10 {
            system.step();
        }

        system.attack_close_perps();
        system.step();
        log::debug!(
            "Attacker wallet before closing spot up: {:?}",
            system.attacker_wallet
        );
        system.attacker_end_spot_up();
        log::debug!(
            "Attacker wallet after closing spot up: {:?}",
            system.attacker_wallet
        );

        for _ in 0..10 {
            system.step();
        }

        Ok(())
    }
}
