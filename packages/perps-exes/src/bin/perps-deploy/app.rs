use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result};
use cosmos::{Address, Cosmos, CosmosNetwork, HasAddress, HasAddressType, Wallet};
use msg::contracts::pyth_bridge::PythMarketPriceFeeds;
use msg::prelude::MarketId;
use perps_exes::config::{ChainConfig, ConfigTestnet, DeploymentConfigTestnet, PythConfig};

use crate::{cli::Opt, faucet::Faucet, tracker::Tracker};

/// Basic app configuration for talking to a chain
pub(crate) struct BasicApp {
    pub(crate) cosmos: Cosmos,
    pub(crate) wallet: Wallet,
    pub(crate) chain_config: ChainConfig,
    pub(crate) network: CosmosNetwork,
}

/// Complete app for talking to a testnet with a specific contract family
pub(crate) struct App {
    pub(crate) basic: BasicApp,
    pub(crate) wallet_manager: Address,
    pub(crate) trading_competition: bool,
    pub(crate) tracker: Tracker,
    pub(crate) faucet: Faucet,
    pub(crate) dev_settings: bool,
    pub(crate) default_market_ids: Vec<MarketId>,
    pub(crate) price_source: PriceSourceConfig,
    pub(crate) market_config: PathBuf,
}

#[derive(Clone)]
pub(crate) enum PriceSourceConfig {
    Pyth(PythInfo),
    Wallet(Address),
}

/// Complete app for mainnet
pub(crate) struct AppMainnet {
    pub(crate) cosmos: Cosmos,
    pub(crate) wallet: Wallet,
    pub(crate) pyth: PythInfo,
}

#[derive(Clone, Debug)]
pub(crate) struct PythInfo {
    pub address: Address,
    pub markets: HashMap<MarketId, PythMarketPriceFeeds>,
    pub update_age_tolerance: u32,
}

impl Opt {
    pub(crate) async fn connect(&self, network: CosmosNetwork) -> Result<Cosmos> {
        let mut builder = network.builder().await?;
        if let Some(grpc) = &self.cosmos_grpc {
            builder.grpc_url = grpc.clone();
        }
        if let Some(rpc) = &self.cosmos_rpc {
            builder.config.rpc_url = Some(rpc.clone());
        }
        if let Some(chain_id) = &self.cosmos_chain_id {
            builder.chain_id = chain_id.clone();
        }
        log::info!("Connecting to {}", builder.grpc_url);

        builder.build().await
    }

    fn get_wallet(&self, network: CosmosNetwork) -> Result<Wallet> {
        Ok(self
            .wallet
            .context("No wallet provided on CLI")?
            .for_chain(network.get_address_type()))
    }

    pub(crate) async fn load_basic_app(&self, network: CosmosNetwork) -> Result<BasicApp> {
        let cosmos = self.connect(network).await?;
        let wallet = self.get_wallet(network)?;
        let chain_config = ChainConfig::load(self.config_chain.as_ref(), network)?;

        Ok(BasicApp {
            cosmos,
            wallet,
            chain_config,
            network,
        })
    }

    pub(crate) async fn load_app(&self, family: &str) -> Result<App> {
        let config = ConfigTestnet::load(self.config_testnet.as_ref())?;
        let pyth_config = PythConfig::load(self.config_pyth.as_ref())?;
        let partial = config.get_deployment_info(family)?;
        let basic = self.load_basic_app(partial.network).await?;

        let DeploymentConfigTestnet {
            wallet_manager_address,
            trading_competition,
            dev_settings,
            default_market_ids,
            qa_price_updates,
            ..
        } = partial.config;

        let (tracker, faucet) = basic.get_tracker_and_faucet()?;

        // only create pyth_info (with markets etc.) if we have a pyth address and do not specify
        let price_source = if qa_price_updates {
            PriceSourceConfig::Wallet(
                config
                    .qa_wallet
                    .for_chain(partial.network.get_address_type()),
            )
        } else {
            let pyth_address = basic
                .chain_config
                .pyth
                .with_context(|| format!("No Pyth address found for {family}"))?;

            // Pyth config validation
            for (market_id, market_price_feeds) in &pyth_config.markets {
                if market_price_feeds.feeds_usd.is_none() && !market_id.is_notional_usd() {
                    anyhow::bail!(
                        "notional is not USD, so there MUST be a USD price feed. MarketId: {}",
                        market_id
                    );
                }
            }

            PriceSourceConfig::Pyth(PythInfo {
                address: pyth_address,
                markets: pyth_config.markets.clone(),
                update_age_tolerance: pyth_config.update_age_tolerance,
            })
        };

        Ok(App {
            wallet_manager: wallet_manager_address.for_chain(partial.network.get_address_type()),
            trading_competition,
            dev_settings,
            tracker,
            faucet,
            basic,
            default_market_ids,
            price_source,
            market_config: self.market_config.clone(),
        })
    }

    pub(crate) async fn load_app_mainnet(&self, network: CosmosNetwork) -> Result<AppMainnet> {
        let pyth_config = PythConfig::load(self.config_pyth.as_ref())?;
        let chain_config = ChainConfig::load(self.config_chain.as_ref(), network)?;
        let cosmos = self.connect(network).await?;
        let wallet = self.get_wallet(network)?;

        let pyth_address = chain_config
            .pyth
            .with_context(|| format!("No Pyth configuration found for {network}"))?;

        // Pyth config validation
        for (market_id, market_price_feeds) in &pyth_config.markets {
            if market_price_feeds.feeds_usd.is_none() && !market_id.is_notional_usd() {
                anyhow::bail!(
                    "notional is not USD, so there MUST be a USD price feed. MarketId: {}",
                    market_id
                );
            }
        }

        let pyth = PythInfo {
            address: pyth_address,
            markets: pyth_config.markets.clone(),
            update_age_tolerance: pyth_config.update_age_tolerance,
        };

        Ok(AppMainnet {
            pyth,
            cosmos,
            wallet,
        })
    }
}

impl BasicApp {
    pub(crate) fn get_tracker_and_faucet(&self) -> Result<(Tracker, Faucet)> {
        let tracker = self
            .chain_config
            .tracker
            .with_context(|| format!("No tracker found for {}", self.network))?;
        let faucet = self
            .chain_config
            .faucet
            .with_context(|| format!("No faucet found for {}", self.network))?;
        anyhow::ensure!(tracker.get_address_type() == self.network.get_address_type());
        anyhow::ensure!(faucet.get_address_type() == self.network.get_address_type());
        Ok((
            Tracker::from_contract(self.cosmos.make_contract(tracker)),
            Faucet::from_contract(self.cosmos.make_contract(faucet)),
        ))
    }
}
