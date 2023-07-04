use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::DateTime;
use chrono::Utc;
use cosmos::Address;
use cosmos::Cosmos;
use cosmos::HasAddressType;
use cosmos::Wallet;
use cosmwasm_std::Decimal256;
use parking_lot::RwLock;
use perps_exes::config::PythConfig;
use reqwest::Client;
use tokio::sync::Mutex;

use crate::app::factory::{get_factory_info_mainnet, get_factory_info_testnet};
use crate::cli::Opt;
use crate::config::{BotConfig, BotConfigByType, BotConfigTestnet};
use crate::util::helpers::VecWithCurr;
use crate::wallet_manager::ManagedWallet;
use crate::watcher::TaskStatuses;
use crate::watcher::Watcher;

use super::factory::{FactoryInfo, FrontendInfoTestnet};
use super::gas_check::{GasCheckBuilder, GasCheckWallet};

#[derive(Default, serde::Serialize)]
pub(crate) struct GasRecords {
    pub(crate) total: Decimal256,
    pub(crate) entries: VecDeque<GasEntry>,
}

impl GasRecords {
    pub(crate) fn add_entry(&mut self, timestamp: DateTime<Utc>, amount: u128) {
        if let Err(e) = self.add_entry_inner(timestamp, amount) {
            log::error!("Error adding gas record {timestamp}/{amount}: {e:?}");
        }
    }

    fn add_entry_inner(&mut self, timestamp: DateTime<Utc>, amount: u128) -> Result<()> {
        let amount = Decimal256::from_ratio(amount, 1_000_000u32);
        self.total = self.total.checked_add(amount)?;
        self.entries.push_back(GasEntry { timestamp, amount });
        if self.entries.len() > 1000 {
            self.entries.pop_front();
        }
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub(crate) struct GasEntry {
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) amount: Decimal256,
}

pub(crate) struct App {
    factory: RwLock<Arc<FactoryInfo>>,
    frontend_info_testnet: Option<RwLock<Arc<FrontendInfoTestnet>>>,
    pub(crate) cosmos: Cosmos,
    pub(crate) config: BotConfig,
    pub(crate) client: Client,
    pub(crate) bind: SocketAddr,
    pub(crate) statuses: TaskStatuses,
    pub(crate) live_since: DateTime<Utc>,
    pub(crate) gases: RwLock<HashMap<Address, GasRecords>>,
    /// Ensure that the crank and price bots don't try to work at the same time
    pub(crate) crank_lock: Mutex<()>,
    pub endpoints: PythEndpoints,
}

pub(crate) type PythEndpoints = VecWithCurr<String>;

/// Helper data structure for building up an application.
pub(crate) struct AppBuilder {
    pub(crate) app: Arc<App>,
    pub(crate) watcher: Watcher,
    pub(crate) gas_check: GasCheckBuilder,
}

impl Opt {
    async fn make_cosmos(&self, config: &BotConfig) -> Result<Cosmos> {
        let mut builder = config.network.builder().await?;
        if let Some(grpc) = &self.grpc_url {
            builder.grpc_url = grpc.clone();
        }
        if let Some(chain_id) = &self.chain_id {
            builder.chain_id = chain_id.clone();
        }
        if let Some(gas_multiplier) = config.gas_multiplier {
            builder.config.gas_estimate_multiplier = gas_multiplier;
        }
        builder.set_connection_count(config.total_bot_count());
        builder.set_referer_header("https://bots.levana.exchange/".to_owned());
        builder.build().await
    }

    pub(crate) async fn into_app_builder(self) -> Result<AppBuilder> {
        let (config, faucet_bot_runner) = self.get_bot_config()?;
        let client = Client::builder().user_agent("perps-bots").build()?;
        let cosmos = self.make_cosmos(&config).await?;

        let gas_wallet = match &self.sub {
            crate::cli::Sub::Testnet { .. } => {
                Some(self.get_gas_wallet(cosmos.get_address_type())?)
            }
            crate::cli::Sub::Mainnet { .. } => None,
        };

        let (factory, frontend_info_testnet) = match &config.by_type {
            BotConfigByType::Testnet { inner } => {
                let (_, factory, frontend) = get_factory_info_testnet(
                    &cosmos,
                    &client,
                    inner.tracker,
                    inner.faucet,
                    &inner.contract_family,
                    &inner.rpc_nodes,
                )
                .await?;
                (factory, Some(RwLock::new(Arc::new(frontend))))
            }
            BotConfigByType::Mainnet { inner } => (
                get_factory_info_mainnet(&cosmos, inner.factory).await?.1,
                None,
            ),
        };

        let endpoints = VecWithCurr::new(PythConfig::load()?.endpoints.clone());

        let app = App {
            factory: RwLock::new(Arc::new(factory)),
            cosmos,
            config,
            client,
            bind: self.bind,
            statuses: TaskStatuses::default(),
            live_since: Utc::now(),
            gases: RwLock::new(HashMap::new()),
            frontend_info_testnet,
            crank_lock: Mutex::new(()),
            endpoints,
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
        wallet_name: GasCheckWallet,
    ) -> Result<()> {
        self.gas_check
            .add(address, wallet_name, testnet.min_gas, true)
    }

    pub(crate) fn alert_on_low_gas(
        &mut self,
        address: Address,
        wallet_name: GasCheckWallet,
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
        desc: ManagedWallet,
    ) -> Result<Wallet> {
        let wallet = testnet.wallet_manager.get_wallet(desc)?;
        self.refill_gas(testnet, *wallet.address(), GasCheckWallet::Managed(desc))?;
        Ok(wallet)
    }
}

impl App {
    pub(crate) fn get_factory_info(&self) -> Arc<FactoryInfo> {
        self.factory.read().clone()
    }

    pub(crate) fn set_factory_info(&self, info: FactoryInfo) {
        *self.factory.write() = Arc::new(info);
    }

    pub(crate) fn get_frontend_info_testnet(&self) -> Option<Arc<FrontendInfoTestnet>> {
        self.frontend_info_testnet
            .as_ref()
            .map(|x| x.read().clone())
    }

    pub(crate) fn set_frontend_info_testnet(&self, info: FrontendInfoTestnet) -> Result<()> {
        *self
            .frontend_info_testnet
            .as_ref()
            .context("Tried to set frontend_info_testnet with a mainnet config")?
            .write() = Arc::new(info);
        Ok(())
    }
}
