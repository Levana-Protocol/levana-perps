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

use crate::cli::{MainnetOpt, Opt, TestnetOpt};

#[derive(Clone)]
pub(crate) enum BotConfigByType {
    Testnet {
        inner: Arc<BotConfigTestnet>,
    },
    Mainnet {
        factory: Address,
        pyth: PythChainConfig,
        min_gas_crank: u128,
        min_gas_price: u128,
    },
}
impl BotConfigByType {
    pub(crate) fn is_testnet(&self) -> bool {
        match self {
            BotConfigByType::Testnet { .. } => true,
            BotConfigByType::Mainnet { .. } => false,
        }
    }
}

pub(crate) struct BotConfigTestnet {
    pub(crate) tracker: Address,
    pub(crate) faucet: Address,
    pub(crate) pyth: Option<PythChainConfig>,
    pub(crate) price_api: &'static str,
    pub(crate) contract_family: String,
    pub(crate) min_gas: u128,
    pub(crate) min_gas_in_faucet: u128,
    pub(crate) min_gas_in_gas_wallet: u128,
    pub(crate) explorer: &'static str,
}

pub(crate) struct BotConfig {
    pub(crate) by_type: BotConfigByType,
    pub(crate) network: CosmosNetwork,
    pub(crate) price_wallet: Option<Arc<Wallet>>,
    pub(crate) crank_wallet: Option<Wallet>,
    pub(crate) ultra_crank_wallets: Vec<Wallet>,
    pub(crate) wallet_manager: WalletManager,
    pub(crate) balance: bool,
    pub(crate) liquidity_config: Option<LiquidityConfig>,
    pub(crate) utilization_config: Option<UtilizationConfig>,
    pub(crate) trader_config: Option<(usize, TraderConfig)>,
    pub(crate) watcher: WatcherConfig,
    pub(crate) gas_multiplier: Option<f64>,
    pub(crate) rpc_nodes: Vec<Arc<String>>,
    pub(crate) ignore_stale: bool,
    pub(crate) seconds_till_ultra: u32,
    pub(crate) execs_per_price: Option<u32>,
}

impl Opt {
    pub(crate) fn get_bot_config(&self) -> Result<BotConfig> {
        match &self.sub {
            crate::cli::Sub::Testnet { inner } => self.get_bot_config_testnet(inner),
            crate::cli::Sub::Mainnet { inner } => self.get_bot_config_mainnet(inner),
        }
    }

    fn get_bot_config_testnet(&self, testnet: &TestnetOpt) -> Result<BotConfig> {
        let config = ConfigTestnet::load()?;
        let pyth_config = PythConfig::load()?;
        let DeploymentInfo {
            config: partial,
            network,
            wallet_phrase_name,
        } = config.get_deployment_info(&testnet.deployment)?;
        let ChainConfig {
            tracker,
            faucet,
            pyth,
            explorer,
            rpc_nodes,
            mainnet,
        } = ChainConfig::load(network)?;
        let partial = match &testnet.deployment_config {
            Some(s) => serde_yaml::from_str(s)?,
            None => partial,
        };
        Ok(BotConfig {
            by_type: BotConfigByType::Testnet {
                inner: BotConfigTestnet {
                    tracker: tracker.with_context(|| format!("No tracker found for {network}"))?,
                    faucet: faucet.with_context(|| format!("No faucet found for {network}"))?,
                    pyth: pyth.map(|address| PythChainConfig {
                        address,
                        endpoint: pyth_config.endpoint.clone(),
                    }),
                    price_api: &config.price_api,
                    contract_family: testnet.deployment.clone(),
                    min_gas: partial.min_gas,
                    min_gas_in_faucet: partial.min_gas_in_faucet,
                    min_gas_in_gas_wallet: partial.min_gas_in_gas_wallet,
                    explorer: explorer
                        .as_deref()
                        .with_context(|| format!("No explorer found for network {network}"))?,
                }
                .into(),
            }
            .into(),
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
            liquidity_config: if partial.liquidity {
                Some(config.liquidity.clone())
            } else {
                None
            },
            utilization_config: if partial.utilization {
                Some(config.utilization)
            } else {
                None
            },
            trader_config: Some((testnet.traders.unwrap_or(partial.traders), config.trader)),
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

    fn get_bot_config_mainnet(
        &self,
        MainnetOpt {
            factory,
            seed,
            network,
            gas_multiplier,
            min_gas_crank,
            min_gas_price,
            watcher_config,
        }: &MainnetOpt,
    ) -> Result<BotConfig> {
        let chain_config = ChainConfig::load(*network)?;
        let pyth_config = PythConfig::load()?;
        let pyth = PythChainConfig {
            address: chain_config
                .pyth
                .with_context(|| format!("No Pyth contract found for network {network}"))?,
            endpoint: pyth_config.endpoint.clone(),
        };
        let wallet_manager = WalletManager::new(seed.clone(), network.get_address_type())?;
        let price_wallet = wallet_manager.get_wallet("price")?;
        let crank_wallet = wallet_manager.get_wallet("crank")?;
        let watcher = match watcher_config {
            Some(yaml) => serde_yaml::from_str(yaml).context("Invalid watcher config on CLI")?,
            None => WatcherConfig::default(),
        };
        Ok(BotConfig {
            by_type: BotConfigByType::Mainnet {
                factory: *factory,
                pyth,
                min_gas_crank: *min_gas_crank,
                min_gas_price: *min_gas_price,
            }
            .into(),
            network: *network,
            price_wallet: Some(price_wallet.into()),
            crank_wallet: Some(crank_wallet),
            ultra_crank_wallets: vec![],
            wallet_manager,
            balance: false,
            liquidity_config: None,
            utilization_config: None,
            trader_config: None,
            watcher,
            gas_multiplier: *gas_multiplier,
            rpc_nodes: vec![],
            ignore_stale: false,
            seconds_till_ultra: 0,
            execs_per_price: None,
        })
    }
}

impl BotConfig {
    pub(crate) fn get_pyth(&self) -> Option<&PythChainConfig> {
        match &self.by_type {
            BotConfigByType::Testnet { inner } => inner.pyth.as_ref(),
            BotConfigByType::Mainnet { pyth, .. } => Some(pyth),
        }
    }
}
