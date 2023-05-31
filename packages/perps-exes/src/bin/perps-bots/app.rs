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

impl AppBuilder {
    pub(crate) async fn load(&mut self) -> Result<()> {
        self.launch_factory_task()?;

        match &self.app.config.by_type {
            BotConfigByType::Testnet { inner } => {
                let inner = inner.clone();
                self.alert_on_low_gas(inner.faucet, "faucet", inner.min_gas_in_faucet)?;
                if let Some(gas_wallet) = self.get_gas_wallet_address() {
                    self.alert_on_low_gas(gas_wallet, "gas-wallet", inner.min_gas_in_gas_wallet)?;
                }
                self.refill_gas(
                    self.app.config.wallet_manager.get_minter_address(),
                    "wallet-manager",
                )?;
            }
            BotConfigByType::Mainnet { .. } => (),
        }
        if let Some(faucet_bot) = &self.app.faucet_bot {
            let faucet_bot_address = faucet_bot.get_wallet_address();
            self.refill_gas(faucet_bot_address, "faucet-bot")?;
        }

        let price_wallet = self.app.config.price_wallet.clone();
        if let Some(price_wallet) = price_wallet {
            match &self.app.config.by_type {
                BotConfigByType::Testnet { .. } => {
                    self.refill_gas(*price_wallet.address(), "price-bot")?;
                }
                BotConfigByType::Mainnet { inner } => {
                    self.alert_on_low_gas(
                        *price_wallet.address(),
                        "price-bot",
                        inner.min_gas_price,
                    )?;
                }
            }
            self.start_price(price_wallet).await?;
        }

        self.start_crank_bot()?;
        self.start_ultra_crank_bot()?;

        if !self.app.config.ignore_stale {
            self.track_stale()?;
        }

        self.track_stats()?;

        let balance_wallet = self.get_track_wallet("balance")?;
        if self.app.config.balance {
            self.launch_balance(balance_wallet)?;
        }
        self.track_balance()?;

        let liquidity_wallet = self.get_track_wallet("liquidity")?;

        if let Some(liquidity_config) = &self.app.config.liquidity_config {
            self.launch_liquidity(liquidity_wallet, liquidity_config.clone())?;
        }

        let utilization_wallet = self.get_track_wallet("utilization")?;

        if let Some(utilization_config) = self.app.config.utilization_config {
            self.launch_utilization(utilization_wallet, utilization_config)?;
        }

        if let Some((traders, trader_config)) = self.app.config.trader_config {
            for index in 1..=traders {
                let wallet = self.get_track_wallet(format!("Trader #{index}"))?;
                self.launch_trader(wallet, index, trader_config)?;
            }
        }

        Ok(())
    }
}
