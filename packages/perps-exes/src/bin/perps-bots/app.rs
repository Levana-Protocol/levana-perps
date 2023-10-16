mod balance;
mod crank;
pub(crate) mod factory;
pub(crate) mod faucet;
mod gas_check;
mod liquidity;
mod liquidity_transaction;
mod price;
mod stale;
mod stats;
mod stats_alert;
mod total_deposits;
mod trader;
mod types;
mod ultra_crank;
mod utilization;

use anyhow::Result;
use hyper::server::conn::AddrIncoming;
pub(crate) use types::*;

use crate::config::BotConfigByType;

use self::gas_check::GasCheckWallet;

impl AppBuilder {
    pub(crate) async fn start(
        mut self,
        server: hyper::server::Builder<AddrIncoming>,
    ) -> Result<()> {
        let family = match &self.app.config.by_type {
            crate::config::BotConfigByType::Testnet { inner } => inner.contract_family.clone(),
            crate::config::BotConfigByType::Mainnet { inner } => {
                format!("Factory address {}", inner.factory)
            }
        };
        sentry::configure_scope(|scope| scope.set_tag("bot-name", family));

        // Start the tasks that run on all deployments
        self.start_rest_api(server);
        self.start_factory_task()?;
        self.start_crank_bot()?;
        self.start_price()?;
        self.track_stale()?;
        self.track_stats()?;
        self.track_balance()?;

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
                self.start_balance(inner.clone())?;
                self.start_liquidity(inner.clone())?;
                self.start_utilization(inner.clone())?;
                self.start_traders(inner.clone())?;
                self.start_ultra_crank_bot(&inner)?;
            }
            BotConfigByType::Mainnet { inner } => {
                // Launch mainnet tasks
                let mainnet = inner.clone();
                self.start_stats_alert(mainnet.clone())?;

                // Bug in Osmosis, don't run there
                match self.app.cosmos.get_network() {
                    cosmos::CosmosNetwork::OsmosisMainnet
                    | cosmos::CosmosNetwork::OsmosisTestnet
                    | cosmos::CosmosNetwork::OsmosisLocal => (),
                    _ => {
                        self.start_liquidity_transaction_alert(mainnet.clone())?;
                        self.start_total_deposits_alert(mainnet)?;
                    }
                }
            }
        }

        // Gas task must always be launched last so that it includes all wallets specified above
        let gas_check = self.gas_check.build(self.app.clone());
        self.start_gas_task(gas_check)?;

        self.watcher.wait(&self.app).await
    }
}
