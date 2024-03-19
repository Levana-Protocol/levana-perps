use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result};
use cosmos::{
    error::WalletError, Address, AddressHrp, Cosmos, HasAddress, HasAddressHrp, SeedPhrase, Wallet,
};
use msg::{
    contracts::market::spot_price::{PythPriceServiceNetwork, SpotPriceFeed, SpotPriceFeedData},
    prelude::*,
};
use perps_exes::{
    config::{
        ChainConfig, ChainPythConfig, ChainStrideConfig, ConfigTestnet, DeploymentConfigTestnet,
        MarketPriceFeedConfig, PriceConfig,
    },
    PerpsNetwork,
};

use crate::{cli::Opt, faucet::Faucet, tracker::Tracker};

pub(crate) struct LazyWallet(Option<Wallet>);

impl LazyWallet {
    fn new(raw: Option<SeedPhrase>, address_type: AddressHrp) -> Result<Self, WalletError> {
        raw.map(|raw| raw.with_hrp(address_type))
            .transpose()
            .map(LazyWallet)
    }

    fn get(&self) -> Result<&Wallet> {
        self.0.as_ref().context("No wallet provided on CLI")
    }
}

/// Basic app configuration for talking to a chain
pub(crate) struct BasicApp {
    pub(crate) cosmos: Cosmos,
    wallet: LazyWallet,
    pub(crate) chain_config: ChainConfig,
    pub(crate) price_config: PriceConfig,
    pub(crate) network: PerpsNetwork,
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
    pub(crate) config_testnet: ConfigTestnet,
}

#[derive(Clone)]
pub(crate) enum PriceSourceConfig {
    Oracle(OracleInfo),
    Wallet(Address),
}

#[derive(Clone, Debug)]
pub(crate) struct OracleInfo {
    pub pyth: Option<ChainPythConfig>,
    /// Fallback config if not overridden by a market
    pub stride_fallback: Option<ChainStrideConfig>,
    pub markets: HashMap<MarketId, OracleMarketPriceFeeds>,
}

#[derive(Clone, Debug)]
pub(crate) struct OracleMarketPriceFeeds {
    pub feeds: Vec<SpotPriceFeed>,
    pub feeds_usd: Vec<SpotPriceFeed>,
    pub stride_contract_override: Option<Address>,
}

/// Complete app for mainnet
pub(crate) struct AppMainnet {
    pub(crate) cosmos: Cosmos,
    wallet: LazyWallet,
}

impl AppMainnet {
    pub(crate) fn get_wallet(&self) -> Result<&Wallet> {
        self.wallet.get()
    }
}

impl Opt {
    pub(crate) async fn connect(
        &self,
        network: PerpsNetwork,
    ) -> Result<Cosmos, cosmos::error::BuilderError> {
        let mut builder = network.builder().await?;
        if let Some(grpc) = &self.cosmos_grpc {
            builder.set_grpc_url(grpc);
        }
        if let Some(chain_id) = &self.cosmos_chain_id {
            builder.set_chain_id(chain_id.clone());
        }
        builder.set_referer_header(Some("https://querier.levana.finance".to_owned()));
        if let Some(x) = self.cosmos_gas_multiplier {
            builder.set_gas_estimate_multiplier(x);
        }
        log::info!("Connecting to {}", builder.grpc_url());

        builder.build().await
    }

    fn get_lazy_wallet(&self, network: PerpsNetwork) -> Result<LazyWallet, WalletError> {
        LazyWallet::new(self.wallet.clone(), network.get_address_hrp())
    }

    pub(crate) async fn load_basic_app(&self, network: PerpsNetwork) -> Result<BasicApp> {
        let cosmos = self.connect(network).await?;
        let wallet = self.get_lazy_wallet(network)?;
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
            PriceSourceConfig::Wallet(config.qa_wallet.with_hrp(partial.network.get_address_hrp()))
        } else {
            PriceSourceConfig::Oracle(self.get_oracle_info(
                &basic.chain_config,
                &basic.price_config,
                partial.network,
            )?)
        };

        Ok(App {
            wallet_manager: wallet_manager_address.with_hrp(partial.network.get_address_hrp()),
            trading_competition,
            dev_settings,
            tracker,
            faucet,
            basic,
            default_market_ids,
            price_source,
            market_config: self.market_config.clone(),
            config_testnet: config,
        })
    }

    pub fn get_oracle_info(
        &self,
        chain_config: &ChainConfig,
        global_price_config: &PriceConfig,
        network: PerpsNetwork,
    ) -> Result<OracleInfo> {
        let chain_spot_price_config = chain_config
            .spot_price
            .as_ref()
            .with_context(|| format!("No spot price config found for {:?}", network))?;

        let mut markets = HashMap::new();

        let map_feeds = |feed_configs: &[MarketPriceFeedConfig]| -> Result<Vec<SpotPriceFeed>> {
            let mut feeds = vec![];
            for feed_config in feed_configs {
                match feed_config.clone() {
                    MarketPriceFeedConfig::Constant { price, inverted } => {
                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Constant { price },
                            inverted,
                            volatile: None,
                        });
                    }
                    MarketPriceFeedConfig::Sei { denom, inverted } => {
                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Sei { denom },
                            inverted,
                            volatile: None,
                        });
                    }
                    MarketPriceFeedConfig::Stride {
                        denom,
                        inverted,
                        age_tolerance,
                    } => {
                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Stride {
                                denom,
                                age_tolerance_seconds: age_tolerance,
                            },
                            inverted,
                            volatile: None,
                        });
                    }
                    MarketPriceFeedConfig::Pyth { key, inverted } => {
                        let chain_pyth_config = chain_spot_price_config
                            .pyth
                            .as_ref()
                            .context(format!("No pyth config found for {:?}", network))?;

                        let pyth_config = match chain_pyth_config.r#type {
                            PythPriceServiceNetwork::Edge => &global_price_config.pyth.edge,
                            PythPriceServiceNetwork::Stable => &global_price_config.pyth.stable,
                        };

                        let id = pyth_config.feed_ids.get(&key).with_context(|| {
                            format!("No pyth config found for {} on {:?}", key, network)
                        })?;

                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Pyth {
                                id: *id,
                                age_tolerance_seconds: chain_config
                                    .age_tolerance_seconds
                                    .unwrap_or(pyth_config.update_age_tolerance),
                            },
                            inverted,
                            volatile: None,
                        });
                    }
                    MarketPriceFeedConfig::Simple {
                        contract,
                        inverted,
                        age_tolerance,
                    } => {
                        feeds.push(SpotPriceFeed {
                            data: SpotPriceFeedData::Simple {
                                contract: Addr::unchecked(
                                    contract
                                        .raw()
                                        .with_hrp(network.get_address_hrp())
                                        .get_address_string(),
                                ),
                                age_tolerance_seconds: age_tolerance,
                            },
                            inverted,
                            volatile: None,
                        });
                    }
                }
            }

            Ok(feeds)
        };

        for (market_id, price_feed_configs) in global_price_config
            .networks
            .get(&network)
            .with_context(|| format!("No price feed config found for {:?}", network))?
            .iter()
        {
            markets.insert(
                market_id.clone(),
                OracleMarketPriceFeeds {
                    feeds: map_feeds(&price_feed_configs.feeds)?,
                    feeds_usd: map_feeds(&price_feed_configs.feeds_usd)?,
                    stride_contract_override: price_feed_configs.stride_contract,
                },
            );
        }
        Ok(OracleInfo {
            pyth: chain_spot_price_config.pyth.clone(),
            stride_fallback: chain_spot_price_config.stride.clone(),
            markets,
        })
    }

    pub(crate) async fn load_app_mainnet(&self, network: PerpsNetwork) -> Result<AppMainnet> {
        let cosmos = self.connect(network).await?;
        let wallet = self.get_lazy_wallet(network)?;

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
        anyhow::ensure!(tracker.get_address_hrp() == self.network.get_address_hrp());
        anyhow::ensure!(faucet.get_address_hrp() == self.network.get_address_hrp());
        Ok((
            Tracker::from_contract(self.cosmos.make_contract(tracker)),
            Faucet::from_contract(self.cosmos.make_contract(faucet)),
        ))
    }

    pub(crate) fn get_wallet(&self) -> Result<&Wallet> {
        self.wallet.get()
    }
}
