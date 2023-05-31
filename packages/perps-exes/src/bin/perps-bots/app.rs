mod balance;
mod crank;
pub(crate) mod factory;
pub(crate) mod faucet;
mod gas_check;
mod liquidity;
mod price;
mod stale;
mod stats;
mod trader;
mod types;
mod ultra_crank;
mod utilization;

use anyhow::Result;
pub(crate) use types::*;

use crate::config::BotConfigByType;

use self::gas_check::GasCheckWallet;

impl AppBuilder {
    pub(crate) async fn load(&mut self) -> Result<()> {
        // Start the tasks that run on all deployments
        self.launch_factory_task()?;
        self.start_crank_bot()?;
        self.track_stale()?;
        self.track_stats()?;
        self.track_balance()?;
        self.start_price()?;

        match &self.app.config.by_type {
            // Run tasks that can only run in testnet.
            BotConfigByType::Testnet { inner } => {
                // Deal with the borrow checker by not keeping a reference to self borrowed
                let inner = inner.clone();

                // Establish some gas checks
                let faucet_bot_address = inner.faucet_bot.get_wallet_address();
                self.refill_gas(&inner, faucet_bot_address, GasCheckWallet::FaucetBot)?;

                self.alert_on_low_gas(
                    inner.faucet,
                    GasCheckWallet::FaucetContract,
                    inner.min_gas_in_faucet,
                )?;
                if let Some(gas_wallet) = self.get_gas_wallet_address() {
                    self.alert_on_low_gas(
                        gas_wallet,
                        GasCheckWallet::GasWallet,
                        inner.min_gas_in_gas_wallet,
                    )?;
                }
                self.refill_gas(
                    &inner,
                    inner.wallet_manager.get_minter_address(),
                    GasCheckWallet::WalletManager,
                )?;

                // Launch testnet tasks
                self.launch_balance(inner.clone())?;
                self.launch_liquidity(inner.clone())?;
                self.launch_utilization(inner.clone())?;
                self.launch_traders(inner.clone())?;
                self.start_ultra_crank_bot(&inner)?;
            }
            // Nothing to do, no tasks are mainnet-only
            BotConfigByType::Mainnet { .. } => (),
        }

        Ok(())
    }
}
