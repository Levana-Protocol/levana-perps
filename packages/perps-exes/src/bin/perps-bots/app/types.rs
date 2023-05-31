use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;
use cosmos::Address;
use cosmos::Cosmos;
use cosmos::CosmosNetwork;
use cosmos::HasAddressType;
use cosmos::Wallet;
use parking_lot::RwLock;
use reqwest::Client;

use crate::cli::Opt;
use crate::config::{BotConfig, BotConfigByType, BotConfigTestnet};
use crate::watcher::TaskStatuses;
use crate::watcher::Watcher;

use super::factory::get_factory_info;
use super::factory::FactoryInfo;
use super::faucet::FaucetBot;
use super::gas_check::GasCheckBuilder;

pub(crate) type GasRecords = VecDeque<(DateTime<Utc>, u128)>;
pub(crate) struct App {
    factory: RwLock<Arc<FactoryInfo>>,
    pub(crate) frontend_info: FrontendInfo,
    pub(crate) faucet_bot: Option<FaucetBot>,
    pub(crate) cosmos: Cosmos,
    pub(crate) config: BotConfig,
    pub(crate) client: Client,
    pub(crate) bind: SocketAddr,
    pub(crate) statuses: TaskStatuses,
    pub(crate) live_since: DateTime<Utc>,
    pub(crate) gases: RwLock<HashMap<Address, GasRecords>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct FrontendInfo {
    network: CosmosNetwork,
    price_api: &'static str,
    explorer: &'static str,
    maintenance: Option<String>,
}

/// Helper data structure for building up an application.
pub(crate) struct AppBuilder {
    pub(crate) app: Arc<App>,
    pub(crate) watcher: Watcher,
    gas_check: GasCheckBuilder,
}

impl Opt {
    async fn make_cosmos(&self, config: &BotConfig) -> Result<Cosmos> {
        let mut builder = config.network.builder();
        if let Some(grpc) = &self.grpc_url {
            builder.grpc_url = grpc.clone();
        }
        if let Some(chain_id) = &self.chain_id {
            builder.chain_id = chain_id.clone();
        }
        if let Some(gas_multiplier) = config.gas_multiplier {
            builder.config.gas_estimate_multiplier = gas_multiplier;
        }
        builder.set_referer_header("https://bots.levana.exchange/".to_owned());
        builder.build().await
    }

    pub(crate) async fn into_app_builder(self) -> Result<AppBuilder> {
        let config = self.get_bot_config()?;
        let client = Client::builder().user_agent("perps-bots").build()?;
        let cosmos = self.make_cosmos(&config).await?;

        let gas_wallet = match &self.sub {
            crate::cli::Sub::Testnet { .. } => {
                Some(self.get_gas_wallet(cosmos.get_address_type())?)
            }
            crate::cli::Sub::Mainnet { .. } => None,
        };

        let frontend_info = FrontendInfo {
            network: config.network,
            price_api: match &config.by_type {
                BotConfigByType::Testnet { inner } => inner.price_api,
                BotConfigByType::Mainnet { .. } => "MAINNET",
            },
            explorer: match &config.by_type {
                BotConfigByType::Testnet { inner } => inner.explorer,
                BotConfigByType::Mainnet { .. } => "MAINNET",
            },
            maintenance: match &self.sub {
                crate::cli::Sub::Testnet { inner } => inner
                    .maintenance
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .cloned(),
                crate::cli::Sub::Mainnet { .. } => None,
            },
        };

        let factory = get_factory_info(&cosmos, &config, &client).await?.1;
        log::info!("Discovered factory contract: {}", factory.factory);
        if let Some(faucet) = factory.faucet {
            log::info!("Discovered faucet contract: {}", faucet);
        }

        let (faucet_bot, faucet_bot_runner) = match (&self.sub, &config.by_type) {
            (
                crate::cli::Sub::Testnet { inner: cli },
                BotConfigByType::Testnet { inner: config },
            ) => {
                let faucet_bot_wallet = self.get_faucet_bot_wallet(cosmos.get_address_type())?;
                let (x, y) = FaucetBot::new(
                    faucet_bot_wallet,
                    cli.hcaptcha_secret.clone(),
                    config.clone(),
                );
                (Some(x), Some(y))
            }
            (crate::cli::Sub::Mainnet { .. }, BotConfigByType::Mainnet { .. }) => (None, None),
            _ => anyhow::bail!("Invalid CLI/bot config combo"),
        };

        let app = App {
            factory: RwLock::new(Arc::new(factory)),
            frontend_info,
            faucet_bot,
            cosmos,
            config,
            client,
            bind: self.bind,
            statuses: TaskStatuses::default(),
            live_since: Utc::now(),
            gases: RwLock::new(HashMap::new()),
        };
        let app = Arc::new(app);
        let mut builder = AppBuilder {
            app,
            watcher: Watcher::default(),
            gas_check: GasCheckBuilder::new(gas_wallet.map(Arc::new)),
        };
        if let Some(faucet_bot_runner) = faucet_bot_runner {
            builder.launch_faucet_task(faucet_bot_runner);
        }
        Ok(builder)
    }
}

impl AppBuilder {
    /// Track and refill gas to the default gas level
    pub(crate) fn refill_gas(
        &mut self,
        testnet: &BotConfigTestnet,
        address: Address,
        wallet_name: impl Into<String>,
    ) -> Result<()> {
        self.gas_check
            .add(address, wallet_name, testnet.min_gas, true)
    }

    pub(crate) fn alert_on_low_gas(
        &mut self,
        address: Address,
        wallet_name: impl Into<String>,
        min_gas: u128,
    ) -> Result<()> {
        self.gas_check.add(address, wallet_name, min_gas, false)
    }

    pub(crate) fn get_gas_wallet_address(&self) -> Option<Address> {
        self.gas_check.get_wallet_address()
    }

    /// Get a wallet from the wallet manager and track its gas funds.
    pub(crate) fn get_track_wallet(
        &mut self,
        testnet: &BotConfigTestnet,
        wallet_name: impl Into<String>,
    ) -> Result<Wallet> {
        let wallet_name = wallet_name.into();
        let wallet = testnet.wallet_manager.get_wallet(&wallet_name)?;
        self.refill_gas(testnet, *wallet.address(), wallet_name)?;
        Ok(wallet)
    }

    /// Wait for background tasks to complete.
    pub(crate) async fn wait(mut self) -> Result<()> {
        // Gas task must always be launched last so that it includes all wallets specified above
        let gas_check = self.gas_check.build(self.app.clone());
        self.launch_gas_task(gas_check)?;

        self.watcher.wait(&self.app).await
    }
}

impl App {
    pub(crate) fn get_factory_info(&self) -> Arc<FactoryInfo> {
        self.factory.read().clone()
    }

    pub(crate) fn set_factory_info(&self, info: FactoryInfo) {
        *self.factory.write() = Arc::new(info);
    }
}
