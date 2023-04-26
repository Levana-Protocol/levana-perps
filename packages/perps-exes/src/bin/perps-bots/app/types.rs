use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use cosmos::Address;
use cosmos::Cosmos;
use cosmos::CosmosNetwork;
use cosmos::HasAddressType;
use cosmos::Wallet;
use parking_lot::RwLock;
use perps_exes::config::Config;
use perps_exes::prelude::DeploymentConfig;
use reqwest::Client;

use crate::cli::get_deployment_config;
use crate::cli::Opt;
use crate::watcher::TaskStatuses;
use crate::watcher::Watcher;

use super::factory::get_factory_info;
use super::factory::FactoryInfo;
use super::gas_check::GasCheckBuilder;

pub(crate) struct App {
    factory: RwLock<Arc<FactoryInfo>>,
    pub(crate) frontend_info: FrontendInfo,
    pub(crate) faucet_bot: FaucetBot,
    pub(crate) cosmos: Cosmos,
    pub(crate) config: DeploymentConfig,
    pub(crate) client: Client,
    pub(crate) bind: SocketAddr,
    pub(crate) statuses: TaskStatuses,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct FrontendInfo {
    network: CosmosNetwork,
    price_api: &'static str,
    explorer: &'static str,
}

pub(crate) struct FaucetBot {
    pub(crate) wallet: tokio::sync::RwLock<Wallet>,
    pub(crate) hcaptcha_secret: String,
}

/// Helper data structure for building up an application.
pub(crate) struct AppBuilder {
    pub(crate) app: Arc<App>,
    pub(crate) watcher: Watcher,
    gas_check: GasCheckBuilder,
}

impl Opt {
    pub(crate) async fn into_app_builder(self) -> Result<AppBuilder> {
        let opt = self;
        let config = Config::load()?;
        let deployment_config = get_deployment_config(config, &opt)?;
        let config = deployment_config;
        let mut builder = config.network.builder();
        if let Some(grpc) = &opt.grpc_url {
            builder.grpc_url = grpc.clone();
        }
        let cosmos = builder.build().await?;
        let client = Client::builder().user_agent("perps-bots").build()?;

        let faucet_bot_wallet = opt.get_faucet_bot_wallet(cosmos.get_address_type())?;
        let gas_wallet = opt.get_gas_wallet(cosmos.get_address_type())?;

        let frontend_info = FrontendInfo {
            network: config.network,
            price_api: config.price_api,
            explorer: config.explorer,
        };

        let factory = get_factory_info(&cosmos, &config).await?;
        log::info!("Discovered factory contract: {}", factory.factory);
        log::info!("Discovered faucet contract: {}", factory.faucet);

        let app = App {
            factory: RwLock::new(Arc::new(factory)),
            frontend_info,
            faucet_bot: FaucetBot {
                wallet: tokio::sync::RwLock::new(faucet_bot_wallet),
                hcaptcha_secret: opt.hcaptcha_secret,
            },
            cosmos,
            config,
            client,
            bind: opt.bind,
            statuses: TaskStatuses::default(),
        };
        let app = Arc::new(app);
        Ok(AppBuilder {
            app,
            watcher: Watcher::default(),
            gas_check: GasCheckBuilder::new(Arc::new(gas_wallet)),
        })
    }
}

impl AppBuilder {
    /// Track and refill gas to the default gas level
    pub(crate) fn refill_gas(
        &mut self,
        address: Address,
        wallet_name: impl Into<String>,
    ) -> Result<()> {
        self.gas_check
            .add(address, wallet_name, self.app.config.min_gas, true)
    }

    pub(crate) fn alert_on_low_gas(
        &mut self,
        address: Address,
        wallet_name: impl Into<String>,
        min_gas: u128,
    ) -> Result<()> {
        self.gas_check.add(address, wallet_name, min_gas, false)
    }

    pub(crate) fn get_gas_wallet_address(&self) -> Address {
        self.gas_check.get_wallet_address()
    }

    /// Get a wallet from the wallet manager and track its gas funds.
    pub(crate) fn get_track_wallet(&mut self, wallet_name: impl Into<String>) -> Result<Wallet> {
        let wallet_name = wallet_name.into();
        let wallet = self.app.config.wallet_manager.get_wallet(&wallet_name)?;
        self.refill_gas(*wallet.address(), wallet_name)?;
        Ok(wallet)
    }

    /// Wait for background tasks to complete.
    pub(crate) async fn wait(mut self) -> Result<()> {
        // Gas task must always be launched last so that it includes all wallets specified above
        let gas_check = self.gas_check.build(self.app.clone());
        self.launch_gas_task(gas_check).await?;

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
