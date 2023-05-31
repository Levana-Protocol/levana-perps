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

use crate::{config::BotConfigByType, wallet_manager::ManagedWallet};

impl AppBuilder {
    pub(crate) async fn load(&mut self) -> Result<()> {
        // Start the tasks that run on all deployments
        self.launch_factory_task()?;
        self.start_crank_bot()?;
        self.track_stale()?;
        self.track_stats()?;
        self.track_balance()?;
        self.start_price().await?;

        match &self.app.config.by_type {
            BotConfigByType::Testnet { inner } => {
                let inner = inner.clone();

                if let Some(faucet_bot) = &self.app.faucet_bot {
                    let faucet_bot_address = faucet_bot.get_wallet_address();
                    self.refill_gas(&inner, faucet_bot_address, "faucet-bot")?;
                }

                self.alert_on_low_gas(inner.faucet, "faucet", inner.min_gas_in_faucet)?;
                if let Some(gas_wallet) = self.get_gas_wallet_address() {
                    self.alert_on_low_gas(gas_wallet, "gas-wallet", inner.min_gas_in_gas_wallet)?;
                }
                self.refill_gas(
                    &inner,
                    inner.wallet_manager.get_minter_address(),
                    "wallet-manager",
                )?;
                if inner.balance {
                    let balance_wallet = self.get_track_wallet(&inner, ManagedWallet::Balance)?;
                    self.launch_balance(balance_wallet, inner.clone())?;
                }

                if let Some(liquidity_config) = &inner.liquidity_config {
                    let liquidity_wallet =
                        self.get_track_wallet(&inner, ManagedWallet::Liquidity)?;
                    self.launch_liquidity(
                        liquidity_wallet,
                        liquidity_config.clone(),
                        inner.clone(),
                    )?;
                }

                if let Some(utilization_config) = inner.utilization_config {
                    let utilization_wallet =
                        self.get_track_wallet(&inner, ManagedWallet::Utilization)?;
                    self.launch_utilization(utilization_wallet, utilization_config, inner.clone())?;
                }

                if let Some((traders, trader_config)) = inner.trader_config {
                    for index in 1..=traders {
                        let wallet = self.get_track_wallet(&inner, ManagedWallet::Trader(index))?;
                        self.launch_trader(wallet, index, trader_config, inner.clone())?;
                    }
                }
                self.start_ultra_crank_bot(&inner)?;
            }
            BotConfigByType::Mainnet { .. } => (),
        }

        Ok(())
    }
}
