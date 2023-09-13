use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result};
use cosmos::{Address, Cosmos, CosmosNetwork, HasAddress, HasAddressType, Wallet};
use msg::{
    contracts::market::spot_price::{PythPriceServiceNetwork, SpotPriceFeed, SpotPriceFeedData},
    prelude::*,
};
use perps_exes::config::{
    ChainConfig, ChainPythConfig, ChainStrideConfig, ConfigTestnet, DeploymentConfigTestnet,
    MarketPriceFeedConfig, PriceConfig,
};

use crate::{cli::Opt, faucet::Faucet, tracker::Tracker};

/// Basic app configuration for talking to a chain
pub(crate) struct BasicApp {
    pub(crate) cosmos: Cosmos,
    pub(crate) wallet: Wallet,
    pub(crate) chain_config: ChainConfig,
    pub(crate) price_config: PriceConfig,
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
    pub(crate) market_config: PathBuf,
    pub(crate) price_source: PriceSourceConfig,
}

#[derive(Clone)]
pub(crate) enum PriceSourceConfig {
    Oracle(OracleInfo),
    Wallet(Address),
}

#[derive(Clone, Debug)]
pub(crate) struct OracleInfo {
    pub pyth: Option<ChainPythConfig>,
    pub stride: Option<ChainStrideConfig>,
    pub markets: HashMap<MarketId, OracleMarketPriceFeeds>,
}

#[derive(Clone, Debug)]
pub(crate) struct OracleMarketPriceFeeds {
    pub feeds: Vec<SpotPriceFeed>,
    pub feeds_usd: Vec<SpotPriceFeed>,
}

/// Complete app for mainnet
pub(crate) struct AppMainnet {
    pub(crate) cosmos: Cosmos,
    pub(crate) wallet: Wallet,
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
        self.wallet
            .clone()
            .context("No wallet provided on CLI")?
            .for_chain(network.get_address_type())
    }

    pub(crate) async fn load_basic_app(&self, network: CosmosNetwork) -> Result<BasicApp> {
        let cosmos = self.connect(network).await?;
        let wallet = self.get_wallet(network)?;
        let chain_config = ChainConfig::load(self.config_chain.as_ref(), network)?;
        let price_config = PriceConfig::load(self.config_price.as_ref())?;

        Ok(BasicApp {
            cosmos,
            wallet,
            chain_config,
            price_config,
            network,
        })
    }

    pub(crate) async fn load_app(&self, family: &str) -> Result<App> {
        let config = ConfigTestnet::load(self.config_testnet.as_ref())?;
        let price_config = PriceConfig::load(self.config_price.as_ref())?;
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
            PriceSourceConfig::Oracle(self.get_oracle_info(
                &basic.chain_config,
                &basic.price_config,
                family,
            )?)
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

    pub fn get_oracle_info(
        &self,
        chain_config: &ChainConfig,
        global_price_config: &PriceConfig,
        family: &str,
    ) -> Result<OracleInfo> {
        let chain_spot_price_config = chain_config
            .spot_price
            .as_ref()
            .with_context(|| format!("No spot price config found for {family}"))?;

        let mut markets = HashMap::new();

        for (market_id, price_feed_configs) in global_price_config
            .networks
            .get(family)
            .context("No price feed config found for {family}")?
            .iter()
        {
            let mut feeds = vec![];
            let mut feeds_usd = vec![];

            for feed_config in price_feed_configs.feeds.clone() {
                match feed_config {
                    MarketPriceFeedConfig::Constant { price, inverted } => {
                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Constant { price },
                            inverted,
                        });
                    }
                    MarketPriceFeedConfig::Sei { denom, inverted } => {
                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Sei { denom },
                            inverted,
                        });
                    }
                    MarketPriceFeedConfig::Stride { denom, inverted } => {
                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Stride { denom },
                            inverted,
                        });
                    }
                    MarketPriceFeedConfig::Pyth { key, inverted } => {
                        let id_lookup = match chain_spot_price_config.pyth.as_ref() {
                            Some(pyth) => match pyth.r#type {
                                PythPriceServiceNetwork::Edge => &global_price_config.pyth.edge,
                                PythPriceServiceNetwork::Stable => &global_price_config.pyth.stable,
                            },
                            None => bail!("No pyth config found for {family}"),
                        };
                        let id = id_lookup
                            .feed_ids
                            .get(&key)
                            .context("No pyth config found for {id} on {family}")?
                            .clone();

                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Pyth { id },
                            inverted,
                        });
                    }
                }
            }

            markets.insert(
                market_id.clone(),
                OracleMarketPriceFeeds { feeds, feeds_usd },
            );
        }
        Ok(OracleInfo {
            pyth: chain_spot_price_config.pyth.clone(),
            stride: chain_spot_price_config.stride.clone(),
            markets,
        })
    }

    pub(crate) async fn load_app_mainnet(&self, network: CosmosNetwork) -> Result<AppMainnet> {
        let price_config = PriceConfig::load(self.config_price.as_ref())?;
        let chain_config = ChainConfig::load(self.config_chain.as_ref(), network)?;
        let cosmos = self.connect(network).await?;
        let wallet = self.get_wallet(network)?;

        // TODO - get OracleInfo for mainnet

        Ok(AppMainnet { cosmos, wallet })
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
