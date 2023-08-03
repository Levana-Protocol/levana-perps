pub mod defaults;

use std::{collections::HashMap, path::Path};

use cosmos::{Address, CosmosNetwork, RawAddress};
use msg::{
    contracts::{market::config::ConfigUpdate, pyth_bridge::PythMarketPriceFeeds},
    prelude::*,
};

/// Overall configuration of Pyth, for information valid across all chains.
#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PythConfig {
    /// How to calculate price feeds for each market.
    pub markets: HashMap<MarketId, PythMarketPriceFeeds>,
    /// Endpoints to communicate with to get price data
    pub endpoints: Vec<String>,
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
    pub tracker: Option<Address>,
    pub faucet: Option<Address>,
    pub pyth: Option<Address>,
    pub explorer: Option<String>,
    /// Potential RPC endpoints to use
    #[serde(default)]
    pub rpc_nodes: Vec<String>,
    /// Override the gas multiplier
    pub gas_multiplier: Option<f64>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ConfigTestnet {
    deployments: HashMap<String, DeploymentConfigTestnet>,
    overrides: HashMap<String, DeploymentConfigTestnet>,
    pub price_api: String,
    pub liquidity: LiquidityConfig,
    pub utilization: UtilizationConfig,
    pub trader: TraderConfig,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LiquidityConfig {
    /// Min and max per different markets
    pub markets: HashMap<MarketId, LiquidityBounds>,
    /// Lower bound of util ratio, at which point we would withdraw liquidity
    pub min_util_delta: Signed<Decimal256>,
    /// Upper bound of util ratio, at which point we would deposit liquidity
    pub max_util_delta: Signed<Decimal256>,
    /// When we deposit or withdraw, what utilization ratio do we target?
    pub target_util_delta: Signed<Decimal256>,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LiquidityTransactionConfig {
    /// Total number of blocks between which you need to check values
    pub number_of_blocks: u16,
    /// Percentage change of total liqudity below/above which we should alert
    pub liqudity_percentage: Signed<Decimal256>,
    /// Percentage change of total deposits below/above which we should alert
    pub total_deposits_percentage: Signed<Decimal256>,
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct UtilizationConfig {
    /// Lower bound of util ratio, at which point we would open a position
    pub min_util_delta: Signed<Decimal256>,
    /// Upper bound of util ratio, at which point we would close a position
    pub max_util_delta: Signed<Decimal256>,
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TraderConfig {
    /// Upper bound of util ratio, at which point we always close a position
    pub max_util_delta: Signed<Decimal256>,
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
pub struct DeploymentConfigTestnet {
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
    pub traders: u32,
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
    /// Number of seconds before a price update is forced
    #[serde(default = "defaults::max_price_age_secs")]
    pub max_price_age_secs: u32,
    /// Maximum the price can move before we push a price update, e.g. 0.01 means 1%.
    #[serde(default = "defaults::max_allowed_price_delta")]
    pub max_allowed_price_delta: Decimal256,
    #[serde(default = "defaults::price_age_alert_threshold_secs")]
    pub price_age_alert_threshold_secs: u32,
}

fn load_yaml<T: serde::de::DeserializeOwned>(
    static_path: &str,
    static_contents: &[u8],
    runtime_path: Option<impl AsRef<Path>>,
) -> Result<T> {
    match runtime_path {
        Some(path) => {
            let path = path.as_ref();
            let mut file = fs_err::File::open(path)?;
            serde_yaml::from_reader(&mut file)
                .with_context(|| format!("Parse error reading from YAML file {}", path.display()))
        }
        None => serde_yaml::from_slice(static_contents).with_context(|| {
            format!("Parse error reading from compiled-in YAML file {static_path}")
        }),
    }
}

impl ChainConfig {
    pub fn load(config_file: Option<impl AsRef<Path>>, network: CosmosNetwork) -> Result<Self> {
        load_yaml::<HashMap<CosmosNetwork, Self>>(
            "config-chain.yaml",
            include_bytes!("../assets/config-chain.yaml"),
            config_file,
        )?
        .remove(&network)
        .with_context(|| format!("No chain config found for {network}"))
    }
}

impl ConfigTestnet {
    pub fn load(config_file: Option<impl AsRef<Path>>) -> Result<Self> {
        load_yaml(
            "config-testnet.yaml",
            include_bytes!("../assets/config-testnet.yaml"),
            config_file,
        )
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
    pub fn load(config_file: Option<impl AsRef<Path>>) -> Result<Self> {
        load_yaml(
            "config-pyth.yaml",
            include_bytes!("../assets/config-pyth.yaml"),
            config_file,
        )
    }
}

pub struct DeploymentInfo {
    pub config: DeploymentConfigTestnet,
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
    #[serde(default = "defaults::stats_alert")]
    pub stats_alert: TaskConfig,
    #[serde(default = "defaults::ultra_crank")]
    pub ultra_crank: TaskConfig,
    #[serde(default = "defaults::liquidity_transaction_alert")]
    pub liquidity_transaction: TaskConfig,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            retries: defaults::retries(),
            delay_between_retries: defaults::delay_between_retries(),
            balance: TaskConfig {
                delay: Delay::Constant(20),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            gas_check: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            liquidity: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            trader: TaskConfig {
                delay: Delay::Random {
                    low: 120,
                    high: 1200,
                },
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            utilization: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: 120,
                retries: None,
                delay_between_retries: None,
            },
            track_balance: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: 60,
                retries: None,
                delay_between_retries: None,
            },
            crank: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: 60,
                retries: None,
                delay_between_retries: None,
            },
            get_factory: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            price: TaskConfig {
                delay: Delay::Interval(3),
                out_of_date: 30,
                // Intentionally using different defaults to make sure price
                // updates come through quickly. We increase our retries to
                // compensate for the shorter delay.
                retries: Some(20),
                delay_between_retries: Some(1),
            },
            stale: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            stats: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            stats_alert: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            ultra_crank: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
            },
            liquidity_transaction: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: 180,
                retries: None,
                delay_between_retries: None,
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
    /// How many times to retry before giving up, overriding the general watcher
    /// config
    pub retries: Option<usize>,
    /// How many seconds to delay between retries, overriding the general
    /// watcher config
    pub delay_between_retries: Option<u32>,
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Delay {
    Constant(u64),
    Interval(u64),
    Random { low: u64, high: u64 },
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MarketConfigUpdates {
    pub markets: HashMap<MarketId, ConfigUpdate>,
}

impl MarketConfigUpdates {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut file = fs_err::File::open(path)?;
        serde_yaml::from_reader(&mut file)
            .with_context(|| format!("Error loading MarketConfigUpdates from {}", path.display()))
    }
}
