use std::{collections::HashSet, path::PathBuf, sync::Arc};

use cosmos::{Address, CosmosNetwork, HasAddressHrp, Wallet};
use perps_exes::{
    config::{
        ChainConfig, ConfigTestnet, DeploymentInfo, GasAmount, GasDecimals, LiquidityConfig,
        LiquidityTransactionConfig, TraderConfig, UtilizationConfig, WatcherConfig,
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
    pub(crate) min_gas_in_faucet: GasAmount,
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
    pub(crate) low_util_ratio: Decimal256,
    pub(crate) high_util_ratio: Decimal256,
    pub(crate) liquidity_transaction: LiquidityTransactionConfig,
    pub(crate) crank_rewards: Address,
    /// Used for checking RPC health, not for making queries
    pub(crate) rpc_endpoint: Arc<String>,
}

pub(crate) struct BotConfig {
    pub(crate) by_type: BotConfigByType,
    pub(crate) network: CosmosNetwork,
    /// Wallet used to update Pyth oracle contract
    pub(crate) price_wallet: Option<Arc<Wallet>>,
    /// Wallets that are used to perform cranking
    pub(crate) crank_wallets: Vec<Wallet>,
    pub(crate) watcher: WatcherConfig,
    pub(crate) gas_multiplier: Option<f64>,
    pub(crate) max_price_age_secs: u32,
    pub(crate) min_price_age_secs: u32,
    pub(crate) max_allowed_price_delta: Decimal256,
    pub(crate) gas_decimals: GasDecimals,
    pub(crate) http_timeout_seconds: u32,
    /// Default minimum gas amount
    pub(crate) min_gas: GasAmount,
    /// The amount of gas in the gas wallet used to top off other wallets
    pub(crate) min_gas_in_gas_wallet: GasAmount,
    /// Wallet used to refill gas for other wallets
    pub(crate) gas_wallet: Arc<Wallet>,
    pub(crate) ignored_markets: HashSet<MarketId>,
}

impl BotConfig {
    /// Get the desintation wallet for crank rewards.
    pub(crate) fn get_crank_rewards_wallet(&self) -> Option<Address> {
        match &self.by_type {
            BotConfigByType::Testnet { inner: _ } => None,
            BotConfigByType::Mainnet { inner } => Some(inner.crank_rewards),
        }
    }
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
        let http_timeout_seconds = testnet.http_timeout_seconds;
        let config = ConfigTestnet::load(testnet.config_testnet.as_ref())?;
        let DeploymentInfo {
            config: partial,
            network,
            wallet_phrase_name,
        } = config.get_deployment_info(&testnet.deployment)?;
        let ChainConfig {
            tracker,
            faucet,
            spot_price: _,
            explorer,
            rpc_nodes,
            gas_multiplier,
            gas_decimals,
            assets: _,
        } = ChainConfig::load(testnet.config_chain.as_ref(), network)?;
        let partial = match &testnet.deployment_config {
            Some(s) => serde_yaml::from_str(s)?,
            None => partial,
        };

        let faucet_bot_wallet = self.get_faucet_bot_wallet(network.get_address_hrp())?;
        let faucet = faucet.with_context(|| format!("No faucet found for {network}"))?;
        let (faucet_bot, faucet_bot_runner) =
            FaucetBot::new(faucet_bot_wallet, testnet.hcaptcha_secret.clone(), faucet);

        let gas_multiplier = testnet.gas_multiplier.or(gas_multiplier);

        let testnet = BotConfigTestnet {
            tracker: tracker.with_context(|| format!("No tracker found for {network}"))?,
            faucet,
            price_api: config.price_api.clone(),
            contract_family: testnet.deployment.clone(),
            min_gas_in_faucet: partial.min_gas_in_faucet,
            explorer: explorer
                .with_context(|| format!("No explorer found for network {network}"))?,
            ultra_crank_wallets: (0..partial.ultra_crank)
                .map(|index| {
                    self.get_crank_wallet(
                        network.get_address_hrp(),
                        &wallet_phrase_name,
                        index + partial.crank,
                    )
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
                network.get_address_hrp(),
            )?,
            faucet_bot,
            maintenance: testnet
                .maintenance
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned(),
        };
        let gas_wallet = Arc::new(self.get_gas_wallet(network.get_address_hrp())?);
        let config = BotConfig {
            by_type: BotConfigByType::Testnet {
                inner: Arc::new(testnet),
            },
            network,
            crank_wallets: (0..partial.crank)
                .map(|idx| {
                    self.get_crank_wallet(network.get_address_hrp(), &wallet_phrase_name, idx)
                })
                .collect::<Result<_>>()?,
            price_wallet: if partial.price {
                Some(Arc::new(self.get_wallet(
                    network.get_address_hrp(),
                    &wallet_phrase_name,
                    "PRICE",
                )?))
            } else {
                None
            },
            watcher: partial.watcher.clone(),
            gas_multiplier,
            max_price_age_secs: partial.max_price_age_secs,
            min_price_age_secs: partial.min_price_age_secs,
            max_allowed_price_delta: partial.max_allowed_price_delta,
            gas_decimals,
            http_timeout_seconds,
            min_gas: partial.min_gas,
            min_gas_in_gas_wallet: partial.min_gas_in_gas_wallet,
            gas_wallet,
            ignored_markets: self.ignored_markets.iter().cloned().collect(),
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
            min_gas,
            min_gas_refill,
            watcher_config,
            max_price_age_secs,
            min_price_age_secs,
            max_allowed_price_delta,
            low_util_ratio,
            high_util_ratio,
            ltc_num_blocks,
            ltc_total_liqudity_percent,
            ltc_total_deposit_percent,
            http_timeout_seconds,
            crank_rewards,
            rpc_endpoint,
            crank_wallets,
        }: &MainnetOpt,
    ) -> Result<BotConfig> {
        let hrp = network.get_address_hrp();
        let get_wallet = |index| {
            let path = hrp.default_derivation_path_with_index(index);
            let mut seed = seed.clone();
            seed.derivation_path = Some(path);
            seed.with_hrp(hrp)
        };

        let gas_wallet = get_wallet(1)?;
        let price_wallet = get_wallet(2)?;
        let crank_wallets = (0..*crank_wallets)
            .map(|idx| get_wallet(idx + 3))
            .collect::<Result<_, _>>()?;
        let watcher = match watcher_config {
            Some(yaml) => serde_yaml::from_str(yaml).context("Invalid watcher config on CLI")?,
            None => WatcherConfig::default(),
        };
        let gas_decimals = ChainConfig::load(None::<PathBuf>, *network)?.gas_decimals;
        Ok(BotConfig {
            by_type: BotConfigByType::Mainnet {
                inner: BotConfigMainnet {
                    factory: *factory,
                    low_util_ratio: *low_util_ratio,
                    high_util_ratio: *high_util_ratio,
                    liquidity_transaction: LiquidityTransactionConfig {
                        number_of_blocks: *ltc_num_blocks,
                        liqudity_percentage: *ltc_total_liqudity_percent,
                        total_deposits_percentage: *ltc_total_deposit_percent,
                    },
                    crank_rewards: *crank_rewards,
                    rpc_endpoint: Arc::new(rpc_endpoint.clone()),
                }
                .into(),
            },
            network: *network,
            price_wallet: Some(price_wallet.into()),
            crank_wallets,
            watcher,
            gas_multiplier: *gas_multiplier,
            max_price_age_secs: max_price_age_secs
                .unwrap_or_else(perps_exes::config::defaults::max_price_age_secs),
            min_price_age_secs: min_price_age_secs
                .unwrap_or_else(perps_exes::config::defaults::min_price_age_secs),
            max_allowed_price_delta: max_allowed_price_delta
                .unwrap_or_else(perps_exes::config::defaults::max_allowed_price_delta),
            gas_decimals,
            http_timeout_seconds: *http_timeout_seconds,
            min_gas: *min_gas,
            min_gas_in_gas_wallet: *min_gas_refill,
            gas_wallet: Arc::new(gas_wallet),
            ignored_markets: self.ignored_markets.iter().cloned().collect(),
        })
    }
}
