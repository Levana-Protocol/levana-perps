mod defaults;

use std::collections::HashMap;

use cosmos::{Address, CosmosNetwork, RawAddress};
use msg::{contracts::pyth_bridge::PythMarketPriceFeeds, prelude::*};
use once_cell::sync::OnceCell;

/// Overall configuration of Pyth, for information valid across all chains.
#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PythConfig {
    /// How to calculate price feeds for each market.
    pub markets: HashMap<MarketId, PythMarketPriceFeeds>,
    /// Endpoint to communicate with to get price data
    pub endpoint: String,
    /// How old a price to allow, in seconds
    pub update_age_tolerance: u32,
}

/// Configuration for chainwide data.
///
/// This contains information which would be valid for multiple different
/// contract deployments on a single chain.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ChainConfig {
    /// Is this chain a mainnet chain?
    ///
    /// Mainnet chains have additional restrictions, such as not looking up
    /// contract addresses via the tracker. This is for heightened security.
    pub mainnet: bool,
    pub tracker: Option<Address>,
    pub faucet: Option<Address>,
    pub pyth: Option<Address>,
    pub explorer: Option<String>,
    /// Potential RPC endpoints to use
    #[serde(default)]
    pub rpc_nodes: Vec<String>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ConfigTestnet {
    deployments: HashMap<String, BotDeploymentConfigTestnet>,
    overrides: HashMap<String, BotDeploymentConfigTestnet>,
    pub price_api: String,
    pub liquidity: LiquidityConfig,
    pub utilization: UtilizationConfig,
    pub trader: TraderConfig,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ConfigMainnet {
    pub deployments: HashMap<String, BotDeploymentConfigMainnet>,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PythChainConfig {
    pub address: Address,
    pub endpoint: String,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LiquidityConfig {
    /// Min and max per different markets
    pub markets: HashMap<MarketId, LiquidityBounds>,
    /// Lower bound of util ratio, at which point we would withdraw liquidity
    pub min_util: Decimal256,
    /// Upper bound of util ratio, at which point we would deposit liquidity
    pub max_util: Decimal256,
    /// When we deposit or withdraw, what utilization ratio do we target?
    pub target_util: Decimal256,
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct UtilizationConfig {
    /// Lower bound of util ratio, at which point we would open a position
    pub min_util: Decimal256,
    /// Upper bound of util ratio, at which point we would close a position
    pub max_util: Decimal256,
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TraderConfig {
    /// Upper bound of util ratio, at which point we always close a position
    pub max_util: Decimal256,
    /// Minimum borrow fee ratio. If below this, we always open positions.
    pub min_borrow_fee: Decimal256,
    /// Maximum borrow fee ratio. If above this, we always close a position.
    pub max_borrow_fee: Decimal256,
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LiquidityBounds {
    pub min: Collateral,
    pub max: Collateral,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BotDeploymentConfigTestnet {
    #[serde(default)]
    pub crank: bool,
    /// How many ultracrank wallets to set up
    #[serde(default)]
    pub ultra_crank: u32,
    /// How many seconds behind we need to be before we kick in the ultracrank
    #[serde(default = "defaults::seconds_till_ultra")]
    pub seconds_till_ultra: u32,
    pub price: bool,
    pub wallet_manager_address: RawAddress,
    pub price_address: RawAddress,
    #[serde(default)]
    pub dev_settings: bool,
    #[serde(default)]
    pub trading_competition: bool,
    #[serde(default)]
    pub liquidity: bool,
    #[serde(default)]
    pub utilization: bool,
    #[serde(default)]
    pub balance: bool,
    #[serde(default)]
    pub traders: usize,
    pub default_market_ids: Vec<MarketId>,
    #[serde(default)]
    pub ignore_stale: bool,
    #[serde(default)]
    pub execs_per_price: Option<u32>,
    #[serde(default)]
    pub watcher: WatcherConfig,
    /// Minimum gas required in wallet managed by perps bots
    #[serde(default = "defaults::min_gas")]
    pub min_gas: u128,
    /// Minimum gas required in the faucet contract
    #[serde(default = "defaults::min_gas_in_faucet")]
    pub min_gas_in_faucet: u128,
    /// Minimum gas required in the gas wallet
    #[serde(default = "defaults::min_gas_in_gas_wallet")]
    pub min_gas_in_gas_wallet: u128,
    /// Override the gas multiplier
    pub gas_multiplier: Option<f64>,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BotDeploymentConfigMainnet {
    #[serde(default)]
    pub crank: bool,
    pub price: bool,
    #[serde(default)]
    pub execs_per_price: Option<u32>,
    pub network: CosmosNetwork,
    pub wallet_phrase_name: String,
}

const CONFIG_CHAIN_YAML: &[u8] = include_bytes!("../assets/config-chain.yaml");
const CONFIG_TESTNET_YAML: &[u8] = include_bytes!("../assets/config-testnet.yaml");
const CONFIG_MAINNET_YAML: &[u8] = include_bytes!("../assets/config-mainnet.yaml");
const CONFIG_PYTH_YAML: &[u8] = include_bytes!("../assets/config-pyth.yaml");

impl ChainConfig {
    pub fn load(network: CosmosNetwork) -> Result<&'static Self> {
        static CONFIG: OnceCell<HashMap<CosmosNetwork, ChainConfig>> = OnceCell::new();
        CONFIG
            .get_or_try_init(|| {
                serde_yaml::from_slice(CONFIG_CHAIN_YAML)
                    .context("Could not parse config-chain.yaml")
            })?
            .get(&network)
            .with_context(|| format!("No chain config found for {network}"))
    }
}

impl ConfigTestnet {
    pub fn load() -> Result<&'static Self> {
        static CONFIG: OnceCell<ConfigTestnet> = OnceCell::new();
        CONFIG.get_or_try_init(|| {
            serde_yaml::from_slice(CONFIG_TESTNET_YAML)
                .context("Could not parse config-testnet.yaml")
        })
    }

    /// Provide the deployment name, such as osmodev, dragonqa, or seibeta
    pub fn get_deployment_info(&self, deployment: &str) -> Result<DeploymentInfo> {
        let (network, suffix) = parse_deployment(deployment)?;
        let wallet_phrase_name = suffix.to_ascii_uppercase();
        let partial_config = self.deployments.get(suffix).with_context(|| {
            format!(
                "No config found for {}. Valid configs: {}",
                suffix,
                self.deployments
                    .keys()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        let partial = self
            .overrides
            .get(deployment)
            .unwrap_or(partial_config)
            .clone();
        Ok(DeploymentInfo {
            config: partial,
            network,
            wallet_phrase_name,
        })
    }
}

impl PythConfig {
    pub fn load() -> Result<&'static Self> {
        static CONFIG: OnceCell<PythConfig> = OnceCell::new();
        CONFIG.get_or_try_init(|| {
            serde_yaml::from_slice(CONFIG_PYTH_YAML).context("Could not parse config-pyth.yaml")
        })
    }
}

impl ConfigMainnet {
    pub fn load() -> Result<&'static Self> {
        static CONFIG: OnceCell<ConfigMainnet> = OnceCell::new();
        CONFIG.get_or_try_init(|| {
            serde_yaml::from_slice(CONFIG_MAINNET_YAML)
                .context("Could not parse config-mainnet.yaml")
        })
    }

    pub fn get_deployment_info(&self, deployment: &str) -> Result<BotDeploymentConfigMainnet> {
        self.deployments
            .get(deployment)
            .with_context(|| {
                format!(
                    "No config found for {}. Valid configs: {}",
                    deployment,
                    self.deployments
                        .keys()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .cloned()
    }
}

pub struct DeploymentInfo {
    pub config: BotDeploymentConfigTestnet,
    pub network: CosmosNetwork,
    pub wallet_phrase_name: String,
}

/// Parse a deployment name (like dragonbeta) into network and family (like dragonfire and beta).
pub fn parse_deployment(deployment: &str) -> Result<(CosmosNetwork, &str)> {
    const NETWORKS: &[(CosmosNetwork, &str)] = &[
        (CosmosNetwork::OsmosisTestnet, "osmo"),
        (CosmosNetwork::Dragonfire, "dragon"),
        (CosmosNetwork::SeiTestnet, "sei"),
    ];
    for (network, prefix) in NETWORKS {
        if let Some(suffix) = deployment.strip_prefix(prefix) {
            return Ok((*network, suffix));
        }
    }
    Err(anyhow::anyhow!(
        "Could not parse deployment: {}",
        deployment
    ))
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct WatcherConfig {
    /// How many times to retry before giving up
    #[serde(default = "defaults::retries")]
    pub retries: usize,
    /// How many seconds to delay between retries
    #[serde(default = "defaults::delay_between_retries")]
    pub delay_between_retries: u32,
    #[serde(default = "defaults::balance")]
    pub balance: TaskConfig,
    #[serde(default = "defaults::gas_check")]
    pub gas_check: TaskConfig,
    #[serde(default = "defaults::liquidity")]
    pub liquidity: TaskConfig,
    #[serde(default = "defaults::trader")]
    pub trader: TaskConfig,
    #[serde(default = "defaults::utilization")]
    pub utilization: TaskConfig,
    #[serde(default = "defaults::track_balance")]
    pub track_balance: TaskConfig,
    #[serde(default = "defaults::crank")]
    pub crank: TaskConfig,
    #[serde(default = "defaults::get_factory")]
    pub get_factory: TaskConfig,
    #[serde(default = "defaults::price")]
    pub price: TaskConfig,
    #[serde(default = "defaults::stale")]
    pub stale: TaskConfig,
    #[serde(default = "defaults::stats")]
    pub stats: TaskConfig,
    #[serde(default = "defaults::ultra_crank")]
    pub ultra_crank: TaskConfig,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct WatcherConfigMainnet {
    /// How many times to retry before giving up
    #[serde(default = "defaults::retries")]
    pub retries: usize,
    /// How many seconds to delay between retries
    #[serde(default = "defaults::delay_between_retries")]
    pub delay_between_retries: u32,
    #[serde(default = "defaults::gas_check")]
    pub gas_check: TaskConfig,
    #[serde(default = "defaults::track_balance")]
    pub track_balance: TaskConfig,
    #[serde(default = "defaults::crank")]
    pub crank: TaskConfig,
    #[serde(default = "defaults::get_factory")]
    pub get_factory: TaskConfig,
    #[serde(default = "defaults::price")]
    pub price: TaskConfig,
    #[serde(default = "defaults::stale")]
    pub stale: TaskConfig,
    #[serde(default = "defaults::stats")]
    pub stats: TaskConfig,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            retries: 6,
            delay_between_retries: 20,
            balance: TaskConfig {
                delay: Delay::Constant(20),
                out_of_date: 180,
            },
            gas_check: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: 180,
            },
            liquidity: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: 180,
            },
            trader: TaskConfig {
                delay: Delay::Random {
                    low: 120,
                    high: 1200,
                },
                out_of_date: 180,
            },
            utilization: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: 120,
            },
            track_balance: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: 60,
            },
            crank: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: 60,
            },
            get_factory: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: 180,
            },
            price: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: 180,
            },
            stale: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: 180,
            },
            stats: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: 180,
            },
            ultra_crank: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: 180,
            },
        }
    }
}

impl Default for WatcherConfigMainnet {
    fn default() -> Self {
        let watcher = WatcherConfig::default();
        Self {
            retries: watcher.retries,
            delay_between_retries: watcher.delay_between_retries,
            gas_check: watcher.gas_check,
            track_balance: watcher.track_balance,
            crank: watcher.crank,
            get_factory: watcher.get_factory,
            price: watcher.price,
            stale: watcher.stale,
            stats: watcher.stats,
        }
    }
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TaskConfig {
    /// Seconds to delay between runs
    pub delay: Delay,
    /// How many seconds before we should consider the result out of date
    ///
    /// This does not include the delay time
    pub out_of_date: u32,
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Delay {
    Constant(u64),
    Random { low: u64, high: u64 },
}
