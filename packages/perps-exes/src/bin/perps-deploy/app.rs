use std::collections::HashMap;

use anyhow::{Context, Result};
use cosmos::{Address, Cosmos, CosmosNetwork, HasAddress, HasAddressType, Wallet};
use msg::contracts::pyth_bridge::PythMarketPriceFeeds;
use msg::prelude::MarketId;
use perps_exes::config::{ChainConfig, Config, DeploymentConfig};

use crate::{cli::Opt, faucet::Faucet, tracker::Tracker};

/// Basic app configuration for talking to a chain
pub(crate) struct BasicApp {
    pub(crate) cosmos: Cosmos,
    pub(crate) wallet: Wallet,
    pub(crate) chain_config: Option<ChainConfig>,
    pub(crate) network: CosmosNetwork,
}

/// Complete app for talking to a testnet with a specific contract family
pub(crate) struct App {
    pub(crate) basic: BasicApp,
    pub(crate) wallet_manager: Address,
    pub(crate) price_admin: Address,
    pub(crate) trading_competition: bool,
    pub(crate) tracker: Tracker,
    pub(crate) faucet: Faucet,
    pub(crate) dev_settings: bool,
    pub(crate) default_market_ids: Vec<MarketId>,
    pub(crate) pyth_info: Option<PythInfo>,
}

#[derive(Clone, Debug)]
pub(crate) struct PythInfo {
    pub address: Address,
    pub markets: HashMap<MarketId, PythMarketPriceFeeds>,
    pub update_age_tolerance: u32,
}

impl Opt {
    pub(crate) async fn load_basic_app(&self, network: CosmosNetwork) -> Result<BasicApp> {
        let mut builder = network.builder();
        if let Some(grpc) = &self.cosmos_grpc {
            builder.grpc_url = grpc.clone();
        }
        log::info!("Connecting to {}", builder.grpc_url);

        let cosmos = builder.build().await?;
        let wallet = self
            .wallet
            .context("No wallet provided on CLI")?
            .for_chain(network.get_address_type());
        let config = Config::load()?;

        Ok(BasicApp {
            cosmos,
            wallet,
            chain_config: config.chains.get(&network).cloned(),
            network,
        })
    }

    pub(crate) async fn load_app(&self, family: &str) -> Result<App> {
        let config = Config::load()?;
        let partial = config.get_deployment_info(family)?;
        let basic = self.load_basic_app(partial.network).await?;

        let DeploymentConfig {
            wallet_manager_address,
            price_address: price_admin,
            trading_competition,
            dev_settings,
            default_market_ids,
            ..
        } = partial.config;

        let (tracker, faucet) = basic.get_tracker_and_faucet()?;

        let pyth_address = basic
            .chain_config
            .as_ref()
            .and_then(|c| c.pyth.as_ref().map(|p| p.address));

        // only create pyth_info (with markets etc.) if we have a pyth address
        let pyth_info = match pyth_address {
            None => None,
            Some(pyth_address) => {
                // Pyth config validation
                for (market_id, market_price_feeds) in &config.pyth_markets {
                    if market_price_feeds.feeds_usd.is_none() && !market_id.is_notional_usd() {
                        anyhow::bail!(
                            "notional is not USD, so there MUST be a USD price feed. MarketId: {}",
                            market_id
                        );
                    }
                }

                Some(PythInfo {
                    address: pyth_address,
                    markets: config.pyth_markets.clone(),
                    update_age_tolerance: config.pyth_update_age_tolerance,
                })
            }
        };

        Ok(App {
            wallet_manager: wallet_manager_address.for_chain(partial.network.get_address_type()),
            price_admin: price_admin.for_chain(partial.network.get_address_type()),
            trading_competition,
            dev_settings,
            tracker,
            faucet,
            basic,
            default_market_ids,
            pyth_info,
        })
    }
}

pub(crate) fn get_suffix_network(family: &str) -> Result<(&str, CosmosNetwork)> {
    const PREFIXES: [(&str, CosmosNetwork); 3] = [
        ("osmo", CosmosNetwork::OsmosisTestnet),
        ("dragon", CosmosNetwork::Dragonfire),
        ("sei", CosmosNetwork::SeiTestnet),
    ];

    for (prefix, network) in PREFIXES {
        if let Some(suffix) = family.strip_prefix(prefix) {
            return Ok((suffix, network));
        }
    }
    Err(anyhow::anyhow!(
        "Family does not contain known prefix: {family}"
    ))
}

impl BasicApp {
    pub(crate) fn get_tracker_and_faucet(&self) -> Result<(Tracker, Faucet)> {
        let ChainConfig {
            tracker, faucet, ..
        } = self
            .chain_config
            .as_ref()
            .with_context(|| format!("No configuration for chain {}", self.network))?;
        anyhow::ensure!(tracker.get_address_type() == self.network.get_address_type());
        anyhow::ensure!(faucet.get_address_type() == self.network.get_address_type());
        Ok((
            Tracker::from_contract(self.cosmos.make_contract(*tracker)),
            Faucet::from_contract(self.cosmos.make_contract(*faucet)),
        ))
    }
}
