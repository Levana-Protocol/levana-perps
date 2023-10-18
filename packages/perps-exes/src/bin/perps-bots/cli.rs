use std::{net::SocketAddr, path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use cosmos::{Address, AddressType, CosmosNetwork, SeedPhrase, Wallet};
use cosmwasm_std::Decimal256;
use perps_exes::{build_version, config::GasAmount};
use shared::storage::MarketId;

#[derive(clap::Parser)]
#[clap(version = build_version())]
pub(crate) struct Opt {
    #[clap(long, short)]
    verbose: bool,
    #[clap(
        long,
        default_value = "[::]:3000",
        env = "LEVANA_BOTS_BIND",
        global = true
    )]
    pub(crate) bind: SocketAddr,
    /// Sentry client key
    #[arg(short, long, env = "SENTRY_KEY")]
    pub(crate) client_key: Option<String>,
    /// Override the gRPC URL
    #[clap(long, env = "COSMOS_GRPC")]
    pub(crate) grpc_url: Option<String>,
    /// Override the chain ID
    #[clap(long, env = "COSMOS_CHAIN_ID")]
    pub(crate) chain_id: Option<String>,
    /// Override the RPC URL
    #[clap(long, env = "COSMOS_RPC")]
    pub(crate) rpc_url: Option<String>,
    #[clap(subcommand)]
    pub(crate) sub: Sub,
    /// Override the Pyth config file
    #[clap(long, env = "LEVANA_BOTS_PRICE_CONFIG")]
    pub(crate) price_config: Option<PathBuf>,
    /// The stable Pyth endpoint
    #[clap(
        long,
        env = "LEVANA_BOTS_PYTH_ENDPOINT_STABLE",
        default_value = "https://hermes.pyth.network/"
    )]
    pub(crate) pyth_endpoint_stable: String,
    /// The edge Pyth endpoint
    #[clap(
        long,
        env = "LEVANA_BOTS_PYTH_ENDPOINT_EDGE",
        default_value = "https://hermes-beta.pyth.network/"
    )]
    pub(crate) pyth_endpoint_edge: String,
}

#[derive(clap::Parser)]
pub(crate) enum Sub {
    Testnet {
        #[clap(flatten)]
        inner: TestnetOpt,
    },
    Mainnet {
        #[clap(flatten)]
        inner: Box<MainnetOpt>,
    },
}

#[derive(clap::Parser)]
pub(crate) struct TestnetOpt {
    /// hCaptcha secret key
    #[clap(long, env = "LEVANA_BOTS_HCAPTCHA_SECRET")]
    pub(crate) hcaptcha_secret: String,
    /// Maintenance mode to use. Empty string is treated as no maintenance mode.
    #[clap(long, env = "LEVANA_BOTS_MAINTENANCE")]
    pub(crate) maintenance: Option<String>,
    /// Override the number of trading bots to run
    #[clap(long, env = "LEVANA_BOTS_TRADERS")]
    pub(crate) traders: Option<u32>,
    /// Override the contents of the DeploymentConfig in YAML format
    #[clap(long, env = "LEVANA_BOTS_DEPLOYMENT_CONFIG")]
    pub(crate) deployment_config: Option<String>,
    /// Deployment name to use (aka contract family)
    #[clap(long, env = "LEVANA_BOTS_DEPLOYMENT")]
    pub(crate) deployment: String,
    #[clap(long, env = "COSMOS_GAS_MULTIPLIER")]
    pub(crate) gas_multiplier: Option<f64>,
    /// Override testnet config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_TESTNET")]
    pub(crate) config_testnet: Option<PathBuf>,
    /// Override chain config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_CHAIN")]
    pub(crate) config_chain: Option<PathBuf>,
    /// Number of seconds before HTTP connections (especially to Pyth) will time out
    #[clap(long, env = "LEVANA_BOTS_HTTP_TIMEOUT_SECONDS", default_value_t = 10)]
    pub(crate) http_timeout_seconds: u32,
}

#[derive(clap::Parser)]
pub(crate) struct MainnetOpt {
    #[clap(long, env = "LEVANA_BOTS_FACTORY")]
    pub(crate) factory: Address,
    #[clap(long, env = "LEVANA_BOTS_SEED_PHRASE")]
    pub(crate) seed: SeedPhrase,
    #[clap(long, env = "COSMOS_NETWORK")]
    pub(crate) network: CosmosNetwork,
    #[clap(long, env = "COSMOS_GAS_MULTIPLIER")]
    pub(crate) gas_multiplier: Option<f64>,
    #[clap(long, env = "LEVANA_BOTS_MIN_GAS_CRANK", default_value = "100")]
    pub(crate) min_gas_crank: GasAmount,
    #[clap(long, env = "LEVANA_BOTS_MIN_GAS_PRICE", default_value = "100")]
    pub(crate) min_gas_price: GasAmount,
    #[clap(long, env = "LEVANA_BOTS_WATCHER_CONFIG")]
    pub(crate) watcher_config: Option<String>,
    #[clap(long, env = "LEVANA_BOTS_MIN_PRICE_AGE_SECS")]
    pub(crate) min_price_age_secs: Option<u32>,
    #[clap(long, env = "LEVANA_BOTS_MAX_PRICE_AGE_SECS")]
    pub(crate) max_price_age_secs: Option<u32>,
    #[clap(long, env = "LEVANA_BOTS_MAX_ALLOWED_PRICE_DELTA")]
    pub(crate) max_allowed_price_delta: Option<Decimal256>,
    #[clap(long, env = "LEVANA_BOTS_LOW_UTIL_RATIO", default_value = "0.5")]
    pub(crate) low_util_ratio: Decimal256,
    #[clap(long, env = "LEVANA_BOTS_HIGH_UTIL_RATIO", default_value = "0.9")]
    pub(crate) high_util_ratio: Decimal256,
    /// Total number of blocks between which you need to check values
    #[clap(long, env = "LEVANA_BOTS_NUM_BLOCKS", default_value = "600")]
    pub(crate) ltc_num_blocks: u16,
    /// Percentage change of total liqudity below/above which we should alert
    #[clap(long, env = "LEVANA_BOTS_LIQUIDITY_PERCENT", default_value = "10")]
    pub(crate) ltc_total_liqudity_percent: Decimal256,
    /// Percentage change of total deposit below/above which we should alert
    #[clap(long, env = "LEVANA_BOTS_DEPOSIT_PERCENT", default_value = "10")]
    pub(crate) ltc_total_deposit_percent: Decimal256,
    /// Number of seconds before HTTP connections (especially to Pyth) will time out
    #[clap(long, env = "LEVANA_BOTS_HTTP_TIMEOUT_SECONDS", default_value_t = 10)]
    pub(crate) http_timeout_seconds: u32,
    /// Rewards destination wallet
    #[clap(long, env = "LEVANA_BOTS_CRANK_REWARDS")]
    pub(crate) crank_rewards: Address,
    /// List of markets that should be ignored
    #[clap(long, env = "LEVANA_BOTS_IGNORED_MARKETS", value_delimiter = ',')]
    pub(crate) ignored_markets: Vec<MarketId>,
    /// Used for RPC health checks
    #[clap(long, env = "LEVANA_BOTS_RPC_ENDPOINT")]
    pub(crate) rpc_endpoint: String,
}

impl Opt {
    pub(crate) fn init_logger(&self) {
        let env = env_logger::Env::default().default_filter_or(if self.verbose {
            format!(
                "{}=debug,cosmos=debug,levana=debug,info",
                env!("CARGO_CRATE_NAME")
            )
        } else {
            "info".to_owned()
        });
        env_logger::Builder::from_env(env).init();
    }

    pub(crate) fn get_wallet(
        &self,
        address_type: AddressType,
        wallet_phrase_name: &str,
        wallet_type: &str,
    ) -> Result<Wallet> {
        let env_var = format!("LEVANA_BOTS_PHRASE_{wallet_phrase_name}_{wallet_type}");
        let phrase = get_env(&env_var)?;
        let wallet = Wallet::from_phrase(&phrase, address_type)?;
        tracing::info!("Wallet address for {wallet_type}: {wallet}");
        Ok(wallet)
    }

    pub(crate) fn get_wallet_seed(
        &self,
        wallet_phrase_name: &str,
        wallet_type: &str,
    ) -> Result<SeedPhrase> {
        let env_var = format!("LEVANA_BOTS_PHRASE_{wallet_phrase_name}_{wallet_type}");
        let phrase = get_env(&env_var)?;
        SeedPhrase::from_str(&phrase)
    }

    pub(crate) fn get_faucet_bot_wallet(&self, address_type: AddressType) -> Result<Wallet> {
        let env_var = "LEVANA_BOTS_PHRASE_FAUCET";
        let phrase = get_env(env_var)?;
        let wallet = Wallet::from_phrase(&phrase, address_type)?;
        tracing::info!("Wallet address for faucet: {wallet}");
        Ok(wallet)
    }

    /// One shared wallet used for refilling gas to all other wallets.
    pub(crate) fn get_gas_wallet(&self, address_type: AddressType) -> Result<Wallet> {
        let env_var = "LEVANA_BOTS_PHRASE_GAS";
        let phrase = get_env(env_var)?;
        let wallet = Wallet::from_phrase(&phrase, address_type)?;
        tracing::info!("Wallet address for gas: {wallet}");
        Ok(wallet)
    }

    pub(crate) fn get_crank_wallet(
        &self,
        address_type: AddressType,
        wallet_phrase_name: &str,
        index: u32,
    ) -> Result<Wallet> {
        let env_var = format!("LEVANA_BOTS_PHRASE_{}_CRANK", wallet_phrase_name);
        let phrase = get_env(&env_var)?;
        let seed = SeedPhrase::from_str(&phrase)?;
        let wallet = seed
            .derive_cosmos_numbered(index.into())
            .for_chain(address_type)?;
        tracing::info!("Crank bot wallet: {wallet}");
        Ok(wallet)
    }
}

fn get_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Unable to load enviornment variable {key}"))
}
