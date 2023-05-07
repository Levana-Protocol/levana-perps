mod defaults;

use std::{collections::HashMap, sync::Arc};

use cosmos::{Address, CosmosNetwork, RawAddress, Wallet};
use msg::{contracts::pyth_bridge::PythMarketPriceFeeds, prelude::*};
use once_cell::sync::OnceCell;

use crate::wallet_manager::WalletManager;

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    pub chains: HashMap<CosmosNetwork, ChainConfig>,
    pub deployments: HashMap<String, PartialDeploymentConfig>,
    pub overrides: HashMap<String, PartialDeploymentConfig>,
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
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PythChainConfig {
    pub address: Address,
    pub endpoint: String,
}

pub struct DeploymentConfig {
    pub tracker: Address,
    pub faucet: Address,
    pub pyth: Option<PythChainConfig>,
    pub min_gas: u128,
    pub min_gas_in_faucet: u128,
    pub min_gas_in_gas_wallet: u128,
    pub price_api: &'static str,
    pub explorer: &'static str,
    pub contract_family: String,
    pub network: CosmosNetwork,
    pub price_wallet: Option<Arc<Wallet>>,
    pub crank_wallet: Wallet,
    pub wallet_manager: WalletManager,
    pub liquidity: bool,
    pub utilization: bool,
    pub balance: bool,
    pub traders: usize,
    pub liquidity_config: LiquidityConfig,
    pub utilization_config: UtilizationConfig,
    pub trader_config: TraderConfig,
    pub watcher: WatcherConfig,
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
pub struct PartialDeploymentConfig {
    #[serde(default)]
    pub crank: bool,
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
}

const CONFIG_YAML: &[u8] = include_bytes!("../assets/config.yaml");

impl Config {
    pub fn load() -> Result<&'static Self> {
        static CONFIG: OnceCell<Config> = OnceCell::new();
        CONFIG.get_or_try_init(|| {
            serde_yaml::from_slice(CONFIG_YAML).context("Could not parse config.yaml")
        })
    }
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
