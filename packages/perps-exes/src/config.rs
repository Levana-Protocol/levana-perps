use std::collections::HashMap;
use std::sync::Arc;

use cosmos::{Address, CosmosNetwork, RawAddress, Wallet};
use msg::prelude::*;
use once_cell::sync::OnceCell;

use crate::wallet_manager::WalletManager;

#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    pub chains: HashMap<CosmosNetwork, ChainConfig>,
    pub deployments: HashMap<String, PartialDeploymentConfig>,
    pub price_api: String,
    pub min_gas: MinGasConfig,
}

#[derive(serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MinGasConfig {
    pub price: u128,
    pub crank: u128,
    pub nibb: u128,
    pub faucet: u128,
    pub faucet_bot: u128,
    pub liquidity: u128,
    pub utilization: u128,
    pub trader: u128,
}

#[derive(serde::Deserialize, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ChainConfig {
    pub tracker: Address,
    pub faucet: Address,
    pub explorer: String,
}

pub struct DeploymentConfig {
    pub tracker: Address,
    pub faucet: Address,
    pub min_gas: MinGasConfig,
    pub price_api: &'static str,
    pub explorer: &'static str,
    pub contract_family: String,
    pub network: CosmosNetwork,
    pub nibb: Option<Arc<NibbConfig>>,
    pub address_override: Option<AddressOverride>,
    pub price_wallet: Option<Arc<Wallet>>,
    pub crank_wallets: Vec<Arc<Wallet>>,
    pub wallet_manager: WalletManager,
    pub liquidity: bool,
    pub utilization: bool,
    pub traders: usize,
}

#[derive(serde::Deserialize, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PartialDeploymentConfig {
    pub crank: CrankConfig,
    pub price: bool,
    pub nibb: Option<NibbConfig>,
    pub address_override: Option<AddressOverride>,
    pub nibb_address: RawAddress,
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
    pub traders: usize,
}

#[derive(serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AddressOverride {
    pub factory: Address,
    pub faucet: Address,
}

#[derive(serde::Deserialize, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CrankConfig {
    /// How many crank wallets to run simultaneously
    pub bot_count: u32,
}

#[derive(serde::Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct NibbConfig {
    pub markets: HashMap<MarketId, MarketConfig>,
}

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct MarketConfig {
    pub target_mid_funding_rates: Number,
    pub funding_rates_range: Number,
    pub delta_size_threshold: Number,
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
