use anyhow::{Context, Result};
use cosmos::{Address, Cosmos, CosmosNetwork, HasAddress, HasAddressType, Wallet};
use perps_exes::config::{ChainConfig, Config, PartialDeploymentConfig};

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
    pub(crate) nibb: Address,
    pub(crate) price: Address,
    pub(crate) trading_competition: bool,
    pub(crate) tracker: Tracker,
    pub(crate) faucet: Faucet,
    pub(crate) dev_settings: bool,
}

impl Opt {
    pub(crate) async fn load_basic_app(&self, network: CosmosNetwork) -> Result<BasicApp> {
        let mut builder = network.builder();
        if let Some(grpc) = &self.cosmos_grpc {
            builder.grpc_url = grpc.clone();
        }
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
        let (suffix, network) = get_suffix_network(family)?;
        let basic = self.load_basic_app(network).await?;

        let config = Config::load()?;
        let PartialDeploymentConfig {
            nibb_address: nibb,
            price_address: price,
            trading_competition,
            dev_settings,
            ..
        } = config.deployments.get(suffix).with_context(|| {
            format!("No configuration for family {suffix}, user parameter was {family}")
        })?;
        let nibb = match self.nibb_bot {
            None => *nibb,
            Some(nibb) => nibb,
        };
        let (tracker, faucet) = basic.get_tracker_faucet()?;
        Ok(App {
            nibb: nibb.for_chain(network.get_address_type()),
            price: price.for_chain(network.get_address_type()),
            trading_competition: *trading_competition,
            dev_settings: *dev_settings,
            tracker,
            faucet,
            basic,
        })
    }
}

pub(crate) fn get_suffix_network(family: &str) -> Result<(&str, CosmosNetwork)> {
    const PREFIXES: [(&str, CosmosNetwork); 2] = [
        ("osmo", CosmosNetwork::OsmosisTestnet),
        ("dragon", CosmosNetwork::Dragonfire),
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
    pub(crate) fn get_tracker_faucet(&self) -> Result<(Tracker, Faucet)> {
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
