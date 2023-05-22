mod defaults;

use std::collections::HashMap;

use cosmos::{Address, CosmosNetwork, RawAddress};
use msg::{contracts::pyth_bridge::PythMarketPriceFeeds, prelude::*};
use once_cell::sync::OnceCell;

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    pub chains: HashMap<CosmosNetwork, ChainConfig>,
    deployments: HashMap<String, DeploymentConfig>,
    overrides: HashMap<String, DeploymentConfig>,
    pub price_api: String,
    pub pyth_markets: HashMap<MarketId, PythMarketPriceFeeds>,
    pub pyth_update_age_tolerance: u32,
    pub liquidity: LiquidityConfig,
    pub utilization: UtilizationConfig,
    pub trader: TraderConfig,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ChainConfig {
    pub tracker: Address,
    pub faucet: Address,
    pub pyth: Option<PythChainConfig>,
    pub explorer: String,
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
    /// Potential RPC endpoints to use
    pub rpc_nodes: Vec<String>,
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
pub struct DeploymentConfig {
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
}

const CONFIG_YAML: &[u8] = include_bytes!("../assets/config.yaml");

impl Config {
    pub fn load() -> Result<&'static Self> {
        static CONFIG: OnceCell<Config> = OnceCell::new();
        CONFIG.get_or_try_init(|| {
            serde_yaml::from_slice(CONFIG_YAML).context("Could not parse config.yaml")
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

pub struct DeploymentInfo {
    pub config: DeploymentConfig,
    pub network: CosmosNetwork,
    pub wallet_phrase_name: String,
}

fn parse_deployment(deployment: &str) -> Result<(CosmosNetwork, &str)> {
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
