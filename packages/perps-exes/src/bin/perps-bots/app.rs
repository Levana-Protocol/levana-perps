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
mod utilization;

use anyhow::Result;
pub(crate) use types::*;

impl AppBuilder {
    pub(crate) async fn load(&mut self) -> Result<()> {
        self.launch_factory_task()?;

        self.alert_on_low_gas(
            self.app.config.faucet,
            "faucet",
            self.app.config.min_gas_in_faucet,
        )?;
        self.alert_on_low_gas(
            self.get_gas_wallet_address(),
            "gas-wallet",
            self.app.config.min_gas_in_gas_wallet,
        )?;
        self.refill_gas(
            self.app.config.wallet_manager.get_minter_address(),
            "wallet-manager",
        )?;
        let faucet_bot_address = self.app.faucet_bot.get_wallet_address();
        self.refill_gas(faucet_bot_address, "faucet-bot")?;

        let price_wallet = self.app.config.price_wallet.clone();
        if let Some(price_wallet) = price_wallet {
            self.refill_gas(*price_wallet.address(), "price-bot")?;
            self.start_price(price_wallet).await?;
        }

        self.start_crank_bot().await?;

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

        if self.app.config.liquidity {
            self.launch_liquidity(liquidity_wallet)?;
        }

        let utilization_wallet = self.get_track_wallet("utilization")?;

        if self.app.config.utilization {
            self.launch_utilization(utilization_wallet)?;
        }

        for index in 1..=self.app.config.traders {
            let wallet = self.get_track_wallet(format!("Trader #{index}"))?;
            self.launch_trader(wallet, index)?;
        }

        Ok(())
    }
}
