use std::sync::Arc;

use cosmos::{Address, CosmosNetwork, HasAddressType, Wallet};
use perps_exes::{
    config::{
        ChainConfig, ConfigTestnet, DeploymentInfo, LiquidityConfig, PythChainConfig, PythConfig,
        TraderConfig, UtilizationConfig, WatcherConfig,
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
    pub(crate) crank_wallet: Option<Wallet>,
    pub(crate) ultra_crank_wallets: Vec<Wallet>,
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
    pub(crate) rpc_nodes: Vec<Arc<String>>,
    pub(crate) ignore_stale: bool,
    pub(crate) seconds_till_ultra: u32,
    pub(crate) execs_per_price: Option<u32>,
}

impl Opt {
    pub(crate) fn get_bot_config(&self) -> Result<BotConfig> {
        let config = ConfigTestnet::load()?;
        let pyth_config = PythConfig::load()?;
        let DeploymentInfo {
            config: partial,
            network,
            wallet_phrase_name,
        } = config.get_deployment_info(&self.deployment)?;
        let ChainConfig {
            tracker,
            faucet,
            pyth,
            explorer,
            rpc_nodes,
            mainnet,
        } = ChainConfig::load(network)?;
        let partial = match &self.deployment_config {
            Some(s) => serde_yaml::from_str(s)?,
            None => partial,
        };
        Ok(BotConfig {
            tracker: tracker.with_context(|| format!("No tracker found for {network}"))?,
            faucet: faucet.with_context(|| format!("No faucet found for {network}"))?,
            pyth: pyth.map(|address| PythChainConfig {
                address,
                endpoint: pyth_config.endpoint.clone(),
            }),
            min_gas: partial.min_gas,
            min_gas_in_faucet: partial.min_gas_in_faucet,
            min_gas_in_gas_wallet: partial.min_gas_in_gas_wallet,
            price_api: &config.price_api,
            explorer: explorer
                .as_deref()
                .with_context(|| format!("No explorer found for network {network}"))?,
            contract_family: self.deployment.clone(),
            network,
            crank_wallet: if partial.crank {
                Some(self.get_crank_wallet(network.get_address_type(), &wallet_phrase_name, 0)?)
            } else {
                None
            },
            ultra_crank_wallets: (1..=partial.ultra_crank)
                .map(|index| {
                    self.get_crank_wallet(network.get_address_type(), &wallet_phrase_name, index)
                })
                .collect::<Result<_>>()?,
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
            traders: self.traders.unwrap_or(partial.traders),
            liquidity_config: config.liquidity.clone(),
            utilization_config: config.utilization,
            trader_config: config.trader,
            watcher: partial.watcher.clone(),
            gas_multiplier: partial.gas_multiplier,
            rpc_nodes: match &self.rpc_url {
                None => rpc_nodes.iter().map(|x| Arc::new(x.clone())).collect(),
                Some(rpc) => vec![rpc.clone().into()],
            },
            ignore_stale: partial.ignore_stale,
            seconds_till_ultra: partial.seconds_till_ultra,
            execs_per_price: partial.execs_per_price,
        })
    }
}
