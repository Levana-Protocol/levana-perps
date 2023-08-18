use std::sync::Arc;

use cosmos::{Address, CosmosNetwork, HasAddressType, Wallet};
use perps_exes::{
    config::{
        ChainConfig, ConfigTestnet, DeploymentInfo, LiquidityConfig, LiquidityTransactionConfig,
        TraderConfig, UtilizationConfig, WatcherConfig,
    },
    prelude::*,
};

use crate::{
    app::faucet::{FaucetBot, FaucetBotRunner},
    cli::{MainnetOpt, Opt, TestnetOpt},
    wallet_manager::WalletManager,
};

#[derive(Clone)]
pub(crate) enum BotConfigByType {
    Testnet { inner: Arc<BotConfigTestnet> },
    Mainnet { inner: Arc<BotConfigMainnet> },
}

pub(crate) struct BotConfigTestnet {
    pub(crate) tracker: Address,
    pub(crate) faucet: Address,
    pub(crate) price_api: String,
    pub(crate) contract_family: String,
    pub(crate) min_gas: u128,
    pub(crate) min_gas_in_faucet: u128,
    pub(crate) min_gas_in_gas_wallet: u128,
    pub(crate) explorer: String,
    pub(crate) ultra_crank_wallets: Vec<Wallet>,
    pub(crate) liquidity_config: Option<LiquidityConfig>,
    pub(crate) utilization_config: Option<UtilizationConfig>,
    pub(crate) trader_config: Option<(u32, TraderConfig)>,
    pub(crate) ignore_stale: bool,
    pub(crate) rpc_nodes: Vec<Arc<String>>,
    pub(crate) seconds_till_ultra: u32,
    pub(crate) balance: bool,
    pub(crate) wallet_manager: WalletManager,
    pub(crate) faucet_bot: FaucetBot,
    pub(crate) maintenance: Option<String>,
}

pub(crate) struct BotConfigMainnet {
    pub(crate) factory: Address,
    pub(crate) min_gas_crank: u128,
    pub(crate) min_gas_price: u128,
    pub(crate) low_util_ratio: Decimal256,
    pub(crate) high_util_ratio: Decimal256,
    pub(crate) liquidity_transaction: LiquidityTransactionConfig,
}

pub(crate) struct BotConfig {
    pub(crate) by_type: BotConfigByType,
    pub(crate) network: CosmosNetwork,
    pub(crate) price_wallet: Option<Arc<Wallet>>,
    pub(crate) crank_wallet: Option<Wallet>,
    pub(crate) watcher: WatcherConfig,
    pub(crate) gas_multiplier: Option<f64>,
    pub(crate) execs_per_price: Option<u32>,
    pub(crate) max_price_age_secs: u32,
    pub(crate) max_allowed_price_delta: Decimal256,
    pub(crate) price_age_alert_threshold_secs: u32,
}

impl Opt {
    pub(crate) fn get_bot_config(&self) -> Result<(BotConfig, Option<FaucetBotRunner>)> {
        match &self.sub {
            crate::cli::Sub::Testnet { inner } => self.get_bot_config_testnet(inner),
            crate::cli::Sub::Mainnet { inner } => {
                let config = self.get_bot_config_mainnet(inner)?;
                Ok((config, None))
            }
        }
    }

    fn get_bot_config_testnet(
        &self,
        testnet: &TestnetOpt,
    ) -> Result<(BotConfig, Option<FaucetBotRunner>)> {
        let config = ConfigTestnet::load(testnet.config_testnet.as_ref())?;
        let DeploymentInfo {
            config: partial,
            network,
            wallet_phrase_name,
        } = config.get_deployment_info(&testnet.deployment)?;
        let ChainConfig {
            tracker,
            faucet,
            pyth: _,
            explorer,
            rpc_nodes,
            gas_multiplier,
        } = ChainConfig::load(testnet.config_chain.as_ref(), network)?;
        let partial = match &testnet.deployment_config {
            Some(s) => serde_yaml::from_str(s)?,
            None => partial,
        };

        let faucet_bot_wallet = self.get_faucet_bot_wallet(network.get_address_type())?;
        let faucet = faucet.with_context(|| format!("No faucet found for {network}"))?;
        let (faucet_bot, faucet_bot_runner) =
            FaucetBot::new(faucet_bot_wallet, testnet.hcaptcha_secret.clone(), faucet);

        let gas_multiplier = testnet.gas_multiplier.or(gas_multiplier);

        let testnet = BotConfigTestnet {
            tracker: tracker.with_context(|| format!("No tracker found for {network}"))?,
            faucet,
            price_api: config.price_api.clone(),
            contract_family: testnet.deployment.clone(),
            min_gas: partial.min_gas,
            min_gas_in_faucet: partial.min_gas_in_faucet,
            min_gas_in_gas_wallet: partial.min_gas_in_gas_wallet,
            explorer: explorer
                .with_context(|| format!("No explorer found for network {network}"))?,
            ultra_crank_wallets: (1..=partial.ultra_crank)
                .map(|index| {
                    self.get_crank_wallet(network.get_address_type(), &wallet_phrase_name, index)
                })
                .collect::<Result<_>>()?,
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
            rpc_nodes: match &self.rpc_url {
                None => rpc_nodes.iter().map(|x| Arc::new(x.clone())).collect(),
                Some(rpc) => vec![rpc.clone().into()],
            },
            ignore_stale: partial.ignore_stale,
            seconds_till_ultra: partial.seconds_till_ultra,
            balance: partial.balance,
            wallet_manager: WalletManager::new(
                self.get_wallet_seed(&wallet_phrase_name, "WALLET_MANAGER")?,
                network.get_address_type(),
            )?,
            faucet_bot,
            maintenance: testnet
                .maintenance
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned(),
        };
        let config = BotConfig {
            by_type: BotConfigByType::Testnet {
                inner: Arc::new(testnet),
            },
            network,
            crank_wallet: if partial.crank {
                Some(self.get_crank_wallet(network.get_address_type(), &wallet_phrase_name, 0)?)
            } else {
                None
            },
            price_wallet: if partial.price {
                Some(Arc::new(self.get_wallet(
                    network.get_address_type(),
                    &wallet_phrase_name,
                    "PRICE",
                )?))
            } else {
                None
            },
            watcher: partial.watcher.clone(),
            gas_multiplier,
            execs_per_price: partial.execs_per_price,
            max_price_age_secs: partial.max_price_age_secs,
            max_allowed_price_delta: partial.max_allowed_price_delta,
            price_age_alert_threshold_secs: partial.price_age_alert_threshold_secs,
        };

        Ok((config, Some(faucet_bot_runner)))
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
            max_price_age_secs,
            max_allowed_price_delta,
            low_util_ratio,
            high_util_ratio,
            price_age_alert_threshold_secs,
            ltc_num_blocks,
            ltc_total_liqudity_percent,
            ltc_total_deposit_percent,
        }: &MainnetOpt,
    ) -> Result<BotConfig> {
        let price_wallet = seed
            .derive_cosmos_numbered(1)?
            .for_chain(network.get_address_type());
        let crank_wallet = seed
            .derive_cosmos_numbered(2)?
            .for_chain(network.get_address_type());
        let watcher = match watcher_config {
            Some(yaml) => serde_yaml::from_str(yaml).context("Invalid watcher config on CLI")?,
            None => WatcherConfig::default(),
        };
        Ok(BotConfig {
            by_type: BotConfigByType::Mainnet {
                inner: BotConfigMainnet {
                    factory: *factory,
                    min_gas_crank: *min_gas_crank,
                    min_gas_price: *min_gas_price,
                    low_util_ratio: *low_util_ratio,
                    high_util_ratio: *high_util_ratio,
                    liquidity_transaction: LiquidityTransactionConfig {
                        number_of_blocks: *ltc_num_blocks,
                        liqudity_percentage: *ltc_total_liqudity_percent,
                        total_deposits_percentage: *ltc_total_deposit_percent,
                    },
                }
                .into(),
            },
            network: *network,
            price_wallet: Some(price_wallet.into()),
            crank_wallet: Some(crank_wallet),
            watcher,
            gas_multiplier: *gas_multiplier,
            execs_per_price: None,
            max_price_age_secs: max_price_age_secs
                .unwrap_or_else(perps_exes::config::defaults::max_price_age_secs),
            max_allowed_price_delta: max_allowed_price_delta
                .unwrap_or_else(perps_exes::config::defaults::max_allowed_price_delta),
            price_age_alert_threshold_secs: price_age_alert_threshold_secs
                .unwrap_or_else(perps_exes::config::defaults::price_age_alert_threshold_secs),
        })
    }
}

impl BotConfig {
    /// Used to determine how many connections to allow in the pool.
    pub(crate) fn total_bot_count(&self) -> usize {
        self.price_wallet.as_ref().map_or(0, |_| 1)
            + self.crank_wallet.as_ref().map_or(0, |_| 1)
            + self.by_type.total_bot_count()
    }
}

impl BotConfigByType {
    fn total_bot_count(&self) -> usize {
        match self {
            BotConfigByType::Testnet { inner } => inner.total_bot_count(),
            BotConfigByType::Mainnet { inner } => inner.total_bot_count(),
        }
    }
}

impl BotConfigTestnet {
    fn total_bot_count(&self) -> usize {
        // Bit match here in case we add more kinds of bots in the future
        let BotConfigTestnet {
            tracker: _,
            faucet: _,
            price_api: _,
            contract_family: _,
            min_gas: _,
            min_gas_in_faucet: _,
            min_gas_in_gas_wallet: _,
            explorer: _,
            ultra_crank_wallets,
            liquidity_config,
            utilization_config,
            trader_config,
            ignore_stale,
            rpc_nodes: _,
            seconds_till_ultra: _,
            balance,
            wallet_manager: _,
            faucet_bot: _,
            maintenance: _,
        } = self;
        ultra_crank_wallets.len()
            + liquidity_config.as_ref().map_or(0, |_| 1)
            + utilization_config.as_ref().map_or(0, |_| 1)
            + trader_config.as_ref().map_or(0, |x| x.0 as usize)
            + if *ignore_stale { 0 } else { 1 }
            + if *balance { 0 } else { 1 }
            + 5 // just some extra to be safe
    }
}

impl BotConfigMainnet {
    fn total_bot_count(&self) -> usize {
        // Just future proofing in case we add some optional bots in the future
        let BotConfigMainnet {
            factory: _,
            min_gas_crank: _,
            min_gas_price: _,
            low_util_ratio: _,
            high_util_ratio: _,
            liquidity_transaction: _,
        } = self;
        0
    }
}
