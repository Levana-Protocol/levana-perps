mod balance;
mod block_lag;
mod congested;
mod copy_trade;
mod countertrade;
mod crank_run;
pub(crate) mod factory;
pub(crate) mod faucet;
pub(crate) mod gas_check;
mod high_gas;
mod liquidity;
mod liquidity_transaction;
mod price;
mod rpc_health;
mod stale;
mod stats;
mod stats_alert;
mod total_deposits;
mod trader;
mod types;
mod ultra_crank;
mod utilization;

use anyhow::Result;
use cosmos::HasAddressHrp;

use tokio::net::TcpListener;

pub(crate) use types::*;

use crate::config::BotConfigByType;

use self::gas_check::GasCheckWallet;

impl AppBuilder {
    pub(crate) async fn start(mut self, listener: TcpListener) -> Result<()> {
        let family = match &self.app.config.by_type {
            crate::config::BotConfigByType::Testnet { inner } => inner.contract_family.clone(),
            crate::config::BotConfigByType::Mainnet { inner } => {
                format!("Factory address {}", inner.factory)
            }
        };
        sentry::configure_scope(|scope| scope.set_tag("bot-name", family));

        // Start the tasks that run on all deployments
        self.start_factory_task()?;
        self.track_stale()?;
        self.track_block_lag()?;

        if self.app.config.run_copy_trade {
            self.start_copytrading_bot()?;
        }

        if self.app.config.run_optional_services {
            self.track_stats()?;
            self.track_balance()?;
        }

        // These services are tied together closely, see docs on the
        // crank_run module for more explanation.
        if let Some(trigger_crank) = self.start_crank_run()? {
            self.start_price(trigger_crank.clone())?;
        }

        self.alert_on_low_gas(
            self.get_gas_wallet_address(),
            GasCheckWallet::GasWallet,
            self.app.config.min_gas_in_gas_wallet,
        )?;

        if self.app.config.run_countertrade {
            self.start_countertrade_bot()?;
        }

        match &self.app.config.by_type {
            // Run tasks that can only run in testnet.
            BotConfigByType::Testnet { inner } => {
                // Deal with the borrow checker by not keeping a reference to self borrowed
                let inner = inner.clone();

                self.alert_on_low_gas(
                    inner.faucet,
                    GasCheckWallet::FaucetContract,
                    inner.min_gas_in_faucet,
                )?;
                self.refill_gas(
                    inner.wallet_manager.get_minter_address(),
                    GasCheckWallet::WalletManager,
                )?;

                if self.app.config.run_optional_services {
                    // Launch testnet tasks
                    self.start_utilization(inner.clone())?;
                    self.start_ultra_crank_bot(&inner)?;
                    self.start_traders(inner.clone())?;
                    self.start_liquidity(inner.clone())?;
                    self.start_balance(inner.clone())?;
                }
            }
            BotConfigByType::Mainnet { inner } => {
                // Launch mainnet tasks
                let mainnet = inner.clone();

                if self.app.config.run_optional_services {
                    self.start_stats_alert(mainnet.clone())?;
                    self.start_rpc_health(mainnet.clone())?;
                }

                match self.app.cosmos.get_address_hrp().as_str() {
                    "osmo" => self.start_congestion_alert()?,
                    // Bug in Osmosis, don't run there
                    _ => {
                        if self.app.config.run_optional_services {
                            self.start_liquidity_transaction_alert(mainnet.clone())?;
                            self.start_total_deposits_alert(mainnet)?;
                        }
                    }
                }
            }
        }

        // Gas task must always be launched last so that it includes all wallets specified above
        let gas_check = self.gas_check.build(self.app.clone());
        self.start_gas_task(gas_check)?;

        // Start waiting on all tasks. This function internally is responsible
        // for launching the final task: the REST API
        self.watcher.wait(self.app, listener).await
    }
}
