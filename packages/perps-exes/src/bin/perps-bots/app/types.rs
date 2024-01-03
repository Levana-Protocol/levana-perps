use std::collections::{HashMap, VecDeque};
use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use chrono::DateTime;
use chrono::Utc;
use cosmos::{Address, HasAddress};
use cosmos::{Coin, Cosmos};
use cosmos::{DynamicGasMultiplier, Wallet};
use cosmwasm_std::Decimal256;
use parking_lot::Mutex;
use perps_exes::config::GasAmount;
use perps_exes::prelude::PriceBaseInQuote;
use reqwest::Client;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::app::factory::{get_factory_info_mainnet, get_factory_info_testnet};
use crate::cli::Opt;
use crate::config::{BotConfig, BotConfigByType, BotConfigTestnet};
use crate::wallet_manager::ManagedWallet;
use crate::watcher::Watcher;

use super::factory::{FactoryInfo, FrontendInfoTestnet};
use super::gas_check::{GasCheckBuilder, GasCheckWallet};
use super::price::pyth_market_hours::PythMarketHours;

#[derive(serde::Serialize)]
pub(crate) struct GasRecords {
    pub(crate) total: GasAmount,
    pub(crate) entries: VecDeque<GasEntry>,
    pub(crate) wallet_type: GasCheckWallet,
    pub(crate) usage_per_hour: GasAmount,
}

impl GasRecords {
    pub(crate) fn add_entry(&mut self, timestamp: DateTime<Utc>, amount: GasAmount) {
        if let Err(e) = self.add_entry_inner(timestamp, amount) {
            tracing::error!("Error adding gas record {timestamp}/{amount}: {e:?}");
        }
    }

    fn add_entry_inner(&mut self, timestamp: DateTime<Utc>, amount: GasAmount) -> Result<()> {
        self.total = GasAmount(self.total.0.checked_add(amount.0)?);
        self.entries.push_back(GasEntry { timestamp, amount });
        if self.entries.len() > 1000 {
            self.entries.pop_front();
        }
        // Lets compute usage per hour
        let timestamp_before_hour = Utc::now() - Duration::from_secs(1);
        let entries_since_hour = self
            .entries
            .iter()
            .filter(|item| item.timestamp >= timestamp_before_hour);
        self.usage_per_hour = entries_since_hour.map(|item| item.amount).sum();
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub(crate) struct GasEntry {
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) amount: GasAmount,
}

#[derive(serde::Serialize)]
pub(crate) struct GasSingleEntry {
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) amount: BigDecimal,
}

#[derive(Serialize, Debug)]
pub(crate) struct FundsCoin {
    denom: String,
    pub(crate) amount: BigDecimal,
}

impl TryFrom<Coin> for FundsCoin {
    type Error = anyhow::Error;

    fn try_from(value: Coin) -> Result<Self> {
        let amount = BigDecimal::from_str(&value.amount)?;
        Ok(FundsCoin {
            denom: value.denom,
            amount,
        })
    }
}

#[derive(serde::Serialize)]
pub(crate) struct FundUsed {
    pub(crate) total: BigDecimal,
    pub(crate) entries: VecDeque<GasSingleEntry>,
    pub(crate) usage_per_hour: BigDecimal,
}

impl FundUsed {
    pub(crate) fn add_entry(&mut self, timestamp: DateTime<Utc>, amount: BigDecimal) {
        if let Err(e) = self.add_entry_inner(timestamp, amount) {
            tracing::error!("Error adding funds used during {timestamp} {e:?}");
        }
    }

    fn add_entry_inner(&mut self, timestamp: DateTime<Utc>, amount: BigDecimal) -> Result<()> {
        self.entries.push_back(GasSingleEntry { timestamp, amount });
        if self.entries.len() > 1000 {
            self.entries.pop_front();
        }
        let timestamp_before_hour = Utc::now() - Duration::from_secs(60 * 60);
        self.usage_per_hour = self
            .entries
            .iter()
            .filter(|item| item.timestamp >= timestamp_before_hour)
            .map(|item| &item.amount)
            .sum();
        Ok(())
    }
}

pub(crate) struct App {
    factory: RwLock<Arc<FactoryInfo>>,
    frontend_info_testnet: Option<RwLock<Arc<FrontendInfoTestnet>>>,
    pub(crate) cosmos: Cosmos,
    /// A separate Cosmos instance just for gas check due to dynamic gas weirdness.
    ///
    /// On Osmosis mainnet we use a dynamic gas multiplier. Since the multiplier for sending coins in gas check is significantly different than smart contract activities, we keep two different Cosmos values.
    pub(crate) cosmos_gas_check: Cosmos,
    pub(crate) config: BotConfig,
    pub(crate) client: Client,
    pub(crate) live_since: DateTime<Utc>,
    pub(crate) gas_refill: RwLock<HashMap<Address, GasRecords>>,
    pub(crate) funds_used: RwLock<HashMap<Address, FundUsed>>,
    pub(crate) endpoint_stable: String,
    pub(crate) endpoint_edge: String,
    pub(crate) pyth_market_hours: PythMarketHours,
    pub(crate) opt: Opt,
    pub(crate) epoch_last_seen: Mutex<Option<Instant>>,
}

/// Helper data structure for building up an application.
pub(crate) struct AppBuilder {
    pub(crate) app: Arc<App>,
    pub(crate) watcher: Watcher,
    pub(crate) gas_check: GasCheckBuilder,
}

impl Opt {
    async fn make_cosmos(&self, config: &BotConfig) -> Result<Cosmos> {
        let mut builder = config.network.builder().await?;
        tracing::info!("Creating connection to network {}", config.network);
        if let Some(grpc) = &self.grpc_url {
            tracing::info!("Overriding gRPC URL to: {grpc}");
            builder.set_grpc_url(grpc);
        }
        for fallback in &self.grpc_fallbacks {
            builder.add_grpc_fallback_url(fallback);
        }
        if let Some(chain_id) = &self.chain_id {
            tracing::info!("Overriding chain ID to: {chain_id}");
            builder.set_chain_id(chain_id.clone());
        }
        match &config.gas_multiplier {
            Some(x) => {
                tracing::info!("Setting static gas multiplier value of {x}");
                builder.set_gas_estimate_multiplier(*x);
            }
            None => {
                let x = Default::default();
                tracing::info!("Setting dynamic gas multiplier config: {x:?}");
                builder.set_dynamic_gas_estimate_multiplier(x);
            }
        }
        builder.set_referer_header(Some("https://bots.levana.exchange/".to_owned()));
        builder.build().await.map_err(|e| e.into())
    }

    pub(crate) async fn into_app_builder(self) -> Result<AppBuilder> {
        let (config, faucet_bot_runner) = self.get_bot_config()?;
        let client = Client::builder()
            .user_agent("perps-bots")
            .timeout(Duration::from_secs(config.http_timeout_seconds.into()))
            .build()?;
        let cosmos = self.make_cosmos(&config).await?;

        let cosmos_gas_check = {
            // We do gas transfers less frequently, and we know that they require a higher multiplier. Start off immediately with the larger numbers and a bigger step-up size.
            let x = DynamicGasMultiplier {
                initial: 2.5,
                step_up: 0.5,
                ..Default::default()
            };
            tracing::info!("For gas check, using the following dynamic parameters: {x:?}");
            cosmos.clone().with_dynamic_gas(x)
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
                    &config.ignored_markets,
                )
                .await?;
                (factory, Some(RwLock::new(Arc::new(frontend))))
            }
            BotConfigByType::Mainnet { inner } => (
                get_factory_info_mainnet(&cosmos, inner.factory, &config.ignored_markets)
                    .await?
                    .1,
                None,
            ),
        };

        let opt = self.clone();

        let app = App {
            factory: RwLock::new(Arc::new(factory)),
            cosmos,
            cosmos_gas_check,
            config,
            client,
            live_since: Utc::now(),
            gas_refill: RwLock::new(HashMap::new()),
            funds_used: RwLock::new(HashMap::new()),
            frontend_info_testnet,
            endpoint_stable: self.pyth_endpoint_stable,
            endpoint_edge: self.pyth_endpoint_edge,
            pyth_market_hours: Default::default(),
            opt,
            epoch_last_seen: Mutex::new(None),
        };
        let app = Arc::new(app);
        let mut builder = AppBuilder {
            gas_check: GasCheckBuilder::new(app.config.gas_wallet.clone()),
            app,
            watcher: Watcher::default(),
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
        address: Address,
        wallet_name: GasCheckWallet,
    ) -> Result<()> {
        self.gas_check
            .add(address, wallet_name, self.app.config.min_gas, true)
    }

    pub(crate) fn alert_on_low_gas(
        &mut self,
        address: Address,
        wallet_name: GasCheckWallet,
        min_gas: GasAmount,
    ) -> Result<()> {
        self.gas_check.add(address, wallet_name, min_gas, false)
    }

    pub(crate) fn get_gas_wallet_address(&self) -> Address {
        self.app.config.gas_wallet.get_address()
    }

    /// Get a wallet from the wallet manager and track its gas funds.
    pub(crate) fn get_track_wallet(
        &mut self,
        testnet: &BotConfigTestnet,
        desc: ManagedWallet,
    ) -> Result<Wallet> {
        let wallet = testnet.wallet_manager.get_wallet(desc)?;
        self.refill_gas(wallet.get_address(), GasCheckWallet::Managed(desc))?;
        Ok(wallet)
    }
}

impl App {
    #[tracing::instrument(skip_all)]
    pub(crate) async fn get_factory_info(&self) -> Arc<FactoryInfo> {
        self.factory.read().await.clone()
    }

    pub(crate) async fn set_factory_info(&self, info: FactoryInfo) {
        *self.factory.write().await = Arc::new(info);
    }

    pub(crate) async fn get_frontend_info_testnet(&self) -> Option<Arc<FrontendInfoTestnet>> {
        if let Some(x) = self.frontend_info_testnet.as_ref() {
            Some(x.read().await.clone())
        } else {
            None
        }
    }

    pub(crate) async fn set_frontend_info_testnet(&self, info: FrontendInfoTestnet) -> Result<()> {
        *self
            .frontend_info_testnet
            .as_ref()
            .context("Tried to set frontend_info_testnet with a mainnet config")?
            .write()
            .await = Arc::new(info);
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum CrankTriggerReason {
    NoPriceOnChain,
    OnChainTooOld {
        on_chain_age: chrono::Duration,
        #[allow(dead_code)]
        off_chain_publish_time: DateTime<Utc>,
        #[allow(dead_code)]
        on_chain_oracle_publish_time: DateTime<Utc>,
    },
    LargePriceDelta {
        on_off_chain_delta: Decimal256,
        #[allow(dead_code)]
        on_chain_oracle_price: PriceBaseInQuote,
        #[allow(dead_code)]
        off_chain_price: PriceBaseInQuote,
    },
    /// Something in the crank queue, either deferred exec or liquifunding, needs a new price.
    CrankNeedsNewPrice {
        #[allow(dead_code)]
        on_chain_oracle_publish_time: DateTime<Utc>,
        work_item: DateTime<Utc>,
    },
    CrankWorkAvailable,
    PriceWillTrigger,
    MoreWorkFound,
    OraclePriceStale,
}

impl Display for CrankTriggerReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            CrankTriggerReason::NoPriceOnChain => f.write_str("No price found on chain"),
            CrankTriggerReason::OnChainTooOld {
                on_chain_age,
                off_chain_publish_time: _,
                on_chain_oracle_publish_time: _,
            } => write!(f, "On chain price too old {on_chain_age:?}"),
            CrankTriggerReason::LargePriceDelta {
                on_off_chain_delta,
                on_chain_oracle_price: _,
                off_chain_price: _,
            } => write!(f, "Large price delta found {on_off_chain_delta}"),
            CrankTriggerReason::CrankNeedsNewPrice {
                on_chain_oracle_publish_time: _,
                work_item: deferred_work_item,
            } => write!(
                f,
                "Deferred work item needs new price (later than {deferred_work_item})"
            ),
            CrankTriggerReason::CrankWorkAvailable => {
                f.write_str("Price bot discovered crank work available")
            }
            CrankTriggerReason::PriceWillTrigger => {
                f.write_str("New price would trigger an action")
            }
            CrankTriggerReason::MoreWorkFound => f.write_str("Crank running discovered more work"),
            CrankTriggerReason::OraclePriceStale => f.write_str("Oracle price is stale"),
        }
    }
}

impl CrankTriggerReason {
    pub(crate) fn needs_price_update(&self) -> bool {
        match self {
            CrankTriggerReason::NoPriceOnChain
            | CrankTriggerReason::OnChainTooOld { .. }
            | CrankTriggerReason::LargePriceDelta { .. }
            | CrankTriggerReason::CrankNeedsNewPrice { .. }
            | CrankTriggerReason::PriceWillTrigger
            | CrankTriggerReason::OraclePriceStale => true,
            CrankTriggerReason::CrankWorkAvailable | CrankTriggerReason::MoreWorkFound => false,
        }
    }
}
