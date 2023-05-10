use std::sync::Arc;

use cosmos::{Address, CosmosNetwork, HasAddressType, Wallet};
use perps_exes::{
    config::{
        ChainConfig, Config, DeploymentInfo, LiquidityConfig, PythChainConfig, TraderConfig,
        UtilizationConfig, WatcherConfig,
    },
    prelude::*,
    wallet_manager::WalletManager,
};

use crate::cli::Opt;

pub(crate) struct BotConfig {
    pub(crate) tracker: Address,
    pub(crate) faucet: Address,
    pub(crate) pyth: Option<PythChainConfig>,
    pub(crate) min_gas: u128,
    pub(crate) min_gas_in_faucet: u128,
    pub(crate) min_gas_in_gas_wallet: u128,
    pub(crate) price_api: &'static str,
    pub(crate) explorer: &'static str,
    pub(crate) contract_family: String,
    pub(crate) network: CosmosNetwork,
    pub(crate) price_wallet: Option<Arc<Wallet>>,
    pub(crate) crank_wallet: Wallet,
    pub(crate) wallet_manager: WalletManager,
    pub(crate) liquidity: bool,
    pub(crate) utilization: bool,
    pub(crate) balance: bool,
    pub(crate) traders: usize,
    pub(crate) liquidity_config: LiquidityConfig,
    pub(crate) utilization_config: UtilizationConfig,
    pub(crate) trader_config: TraderConfig,
    pub(crate) watcher: WatcherConfig,
    pub(crate) gas_multiplier: Option<f64>,
}

impl Opt {
    pub(crate) fn get_bot_config(&self) -> Result<BotConfig> {
        let config = Config::load()?;
        let DeploymentInfo {
            config: partial,
            network,
            wallet_phrase_name,
        } = config.get_deployment_info(&self.deployment)?;
        let ChainConfig {
            tracker,
            faucet,
            explorer,
            pyth,
            watcher,
            min_gas,
            min_gas_in_faucet,
            min_gas_in_gas_wallet,
            gas_multiplier,
        } = config
            .chains
            .get(&network)
            .with_context(|| format!("No chain config found for network {}", network))?;
        Ok(BotConfig {
            tracker: *tracker,
            faucet: *faucet,
            pyth: pyth.clone(),
            min_gas: *min_gas,
            min_gas_in_faucet: *min_gas_in_faucet,
            min_gas_in_gas_wallet: *min_gas_in_gas_wallet,
            price_api: &config.price_api,
            explorer,
            contract_family: self.deployment.clone(),
            network,
            crank_wallet: self.get_crank_wallet(network.get_address_type(), &wallet_phrase_name)?,
            price_wallet: if partial.price {
                Some(Arc::new(self.get_wallet(
                    network.get_address_type(),
                    &wallet_phrase_name,
                    "PRICE",
                )?))
            } else {
                None
            },
            wallet_manager: WalletManager::new(
                self.get_wallet_seed(&wallet_phrase_name, "WALLET_MANAGER")?,
                network.get_address_type(),
            )?,
            balance: partial.balance,
            liquidity: partial.liquidity,
            utilization: partial.utilization,
            traders: partial.traders,
            liquidity_config: config.liquidity.clone(),
            utilization_config: config.utilization,
            trader_config: config.trader,
            watcher: watcher.clone(),
            gas_multiplier: *gas_multiplier,
        })
    }
}
