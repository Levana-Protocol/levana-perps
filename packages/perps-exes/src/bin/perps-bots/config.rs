use std::{collections::HashSet, sync::Arc};

use cosmos::{Address, HasAddressHrp, Wallet};
use perps_exes::{
    config::{
        ChainConfig, ConfigTestnet, DeploymentInfo, GasAmount, GasDecimals, LiquidityConfig,
        LiquidityTransactionConfig, TraderConfig, UtilizationConfig, WatcherConfig,
    },
    prelude::*,
    PerpsNetwork,
};

use crate::{
    app::{
        faucet::{FaucetBot, FaucetBotRunner},
        gas_check::GasCheckBuilder,
        WalletPool, WalletProvider,
    },
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
    pub(crate) ultra_crank_tasks: usize,
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
    /// What gas price on Osmosis to consider the chain congested
    pub(crate) gas_price_congested: f64,
    /// Maximum gas price to allow for transactions
    pub(crate) max_gas_price: f64,
    /// Higher maximum gas price for urgent messages
    pub(crate) higher_max_gas_price: f64,
    /// Higher maximum gas price for *very* urgent messages
    pub(crate) higher_very_high_max_gas_price: f64,
}

#[derive(Clone)]
pub(crate) struct CounterTradeBotConfig {
    /// Contract address
    pub(crate) contract: Address,
}

pub(crate) struct BotConfig {
    pub(crate) by_type: BotConfigByType,
    pub(crate) network: PerpsNetwork,
    /// Should we run the price bot?
    pub(crate) run_price_task: bool,
    /// How many tasks to run cranking wallets
    pub(crate) crank_tasks: usize,
    /// Countertrade Config
    pub(crate) countertrade: Option<CounterTradeBotConfig>,
    /// Wallet used for very high gas situations, derived from price wallet seed
    pub(crate) high_gas_wallet: Option<Arc<Wallet>>,
    pub(crate) watcher: WatcherConfig,
    pub(crate) gas_multiplier: Option<f64>,
    /// Parameters for checking if we need to do a price update or crank
    pub(crate) needs_price_update_params: NeedsPriceUpdateParams,
    pub(crate) gas_decimals: GasDecimals,
    pub(crate) http_timeout_seconds: u32,
    /// Default minimum gas amount
    pub(crate) min_gas: GasAmount,
    pub(crate) min_gas_high_gas_wallet: GasAmount,
    /// The amount of gas in the gas wallet used to top off other wallets
    pub(crate) min_gas_in_gas_wallet: GasAmount,
    /// Wallet used to refill gas for other wallets
    pub(crate) gas_wallet: Arc<Wallet>,
    pub(crate) ignored_markets: HashSet<MarketId>,
    /// How many seconds to ignore errors after an epoch
    pub(crate) ignore_errors_after_epoch_seconds: u32,
    /// Run optional services?
    pub(crate) run_optional_services: bool,
    /// How long to delay after price bot completes before running again
    pub(crate) price_bot_delay: Option<tokio::time::Duration>,
    pub(crate) log_requests: bool,
}

pub(crate) struct NeedsPriceUpdateParams {
    /// How old an on-chain price update can be before we do an update.
    pub(crate) on_chain_publish_time_age_threshold: chrono::Duration,
    /// How large a price delta we need before pushing a price update.
    pub(crate) on_off_chain_price_delta: Decimal256,
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

pub(crate) struct FullBotConfig {
    pub(crate) config: BotConfig,
    pub(crate) faucet_bot: Option<FaucetBotRunner>,
    pub(crate) provider: WalletProvider,
    pub(crate) pool: WalletPool,
    pub(crate) gas_check: GasCheckBuilder,
}

impl Opt {
    pub(crate) fn get_bot_config(&self) -> Result<FullBotConfig> {
        match &self.sub {
            crate::cli::Sub::Testnet { inner } => self.get_bot_config_testnet(inner),
            crate::cli::Sub::Mainnet { inner } => self.get_bot_config_mainnet(inner),
        }
    }

    fn get_bot_config_testnet(&self, testnet: &TestnetOpt) -> Result<FullBotConfig> {
        let http_timeout_seconds = testnet.http_timeout_seconds;
        let config = ConfigTestnet::load_from_opt(testnet.config_testnet.as_deref())?;
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
            age_tolerance_seconds: _,
        } = ChainConfig::load_from_opt(self.config_chain.as_deref(), network)?;

        let seed = self.get_wallet_seed(&wallet_phrase_name)?;
        let mut wallet_provider = WalletProvider::new(seed, network.get_address_hrp());
        let wallet_manager = wallet_provider.next()?;
        let gas_wallet = Arc::new(testnet.gas_phrase.with_hrp(network.get_address_hrp())?);
        let mut gas_check = GasCheckBuilder::new(gas_wallet.clone());

        let faucet = faucet.with_context(|| format!("No faucet found for {network}"))?;
        let wallet_pool = WalletPool::new(
            testnet.pool_wallet_count,
            &mut wallet_provider,
            &mut gas_check,
            partial.min_gas,
        )?;
        let (faucet_bot, faucet_bot_runner) =
            FaucetBot::new(testnet.hcaptcha_secret.clone(), faucet, wallet_pool.clone());
        let countertrade = if let Some(countertrade_contract) = self.countertrade {
            let config = CounterTradeBotConfig {
                contract: countertrade_contract,
            };
            Some(config)
        } else {
            None
        };

        let gas_multiplier = testnet.gas_multiplier.or(gas_multiplier);

        let testnet = BotConfigTestnet {
            tracker: tracker.with_context(|| format!("No tracker found for {network}"))?,
            faucet,
            price_api: config.price_api.clone(),
            contract_family: testnet.deployment.clone(),
            min_gas_in_faucet: partial.min_gas_in_faucet,
            explorer: explorer
                .with_context(|| format!("No explorer found for network {network}"))?,
            ultra_crank_tasks: partial.ultra_crank,
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
            wallet_manager: WalletManager::new(wallet_manager)?,
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
            run_price_task: partial.price,
            crank_tasks: partial.crank,
            high_gas_wallet: None,
            watcher: partial.watcher.clone(),
            gas_multiplier,
            needs_price_update_params: NeedsPriceUpdateParams {
                on_chain_publish_time_age_threshold: chrono::Duration::seconds(
                    partial.max_price_age_secs.into(),
                ),
                on_off_chain_price_delta: partial.max_allowed_price_delta,
            },
            gas_decimals,
            http_timeout_seconds,
            min_gas: partial.min_gas,
            min_gas_high_gas_wallet: partial.min_gas_high_gas_wallet,
            min_gas_in_gas_wallet: partial.min_gas_in_gas_wallet,
            gas_wallet,
            ignored_markets: self.ignored_markets.iter().cloned().collect(),
            // Never used on testnet, just setting a reasonable default
            ignore_errors_after_epoch_seconds: 300,
            run_optional_services: !self.disable_optional_services,
            price_bot_delay: self.price_bot_delay.map(tokio::time::Duration::from_millis),
            log_requests: self.log_requests,
            countertrade,
        };

        Ok(FullBotConfig {
            config,
            faucet_bot: Some(faucet_bot_runner),
            provider: wallet_provider,
            pool: wallet_pool,
            gas_check,
        })
    }

    fn get_bot_config_mainnet(
        &self,
        MainnetOpt {
            factory,
            seed,
            network,
            gas_multiplier,
            min_gas,
            min_gas_high_gas_wallet,
            min_gas_refill,
            max_price_age_secs,
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
            ignore_errors_after_epoch_seconds,
            gas_price_congested,
            max_gas_price,
            higher_max_gas_price,
            very_higher_max_gas_price,
            pool_wallet_count,
        }: &MainnetOpt,
    ) -> Result<FullBotConfig> {
        let mut wallet_provider = WalletProvider::new(seed.clone(), network.get_address_hrp());
        let gas_wallet = Arc::new(wallet_provider.next()?);
        let mut gas_check = GasCheckBuilder::new(gas_wallet.clone());
        let wallet_pool = WalletPool::new(
            *pool_wallet_count,
            &mut wallet_provider,
            &mut gas_check,
            *min_gas,
        )?;

        let high_gas_wallet = match network.get_address_hrp().as_str() {
            "osmo" => Some(Arc::new(wallet_provider.next()?)),
            _ => None,
        };

        let watcher = WatcherConfig::default();
        let gas_decimals =
            ChainConfig::load_from_opt(self.config_chain.as_deref(), *network)?.gas_decimals;
        let config = BotConfig {
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
                    gas_price_congested: *gas_price_congested,
                    max_gas_price: *max_gas_price,
                    higher_max_gas_price: *higher_max_gas_price,
                    higher_very_high_max_gas_price: *very_higher_max_gas_price,
                }
                .into(),
            },
            network: *network,
            run_price_task: true,
            crank_tasks: *crank_wallets,
            high_gas_wallet,
            watcher,
            gas_multiplier: *gas_multiplier,
            needs_price_update_params: NeedsPriceUpdateParams {
                on_chain_publish_time_age_threshold: chrono::Duration::seconds(
                    max_price_age_secs
                        .unwrap_or_else(perps_exes::config::defaults::max_price_age_secs)
                        .into(),
                ),
                on_off_chain_price_delta: max_allowed_price_delta
                    .unwrap_or_else(perps_exes::config::defaults::max_allowed_price_delta),
            },
            gas_decimals,
            http_timeout_seconds: *http_timeout_seconds,
            min_gas: *min_gas,
            min_gas_high_gas_wallet: *min_gas_high_gas_wallet,
            min_gas_in_gas_wallet: *min_gas_refill,
            gas_wallet,
            ignored_markets: self.ignored_markets.iter().cloned().collect(),
            ignore_errors_after_epoch_seconds: *ignore_errors_after_epoch_seconds,
            run_optional_services: !self.disable_optional_services,
            price_bot_delay: self.price_bot_delay.map(tokio::time::Duration::from_millis),
            log_requests: self.log_requests,
            countertrade: self
                .countertrade
                .map(|contract| CounterTradeBotConfig { contract }),
        };
        Ok(FullBotConfig {
            config,
            faucet_bot: None,
            provider: wallet_provider,
            pool: wallet_pool,
            gas_check,
        })
    }
}
