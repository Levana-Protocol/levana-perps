use std::{net::SocketAddr, path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use cosmos::{Address, SeedPhrase};
use cosmwasm_std::Decimal256;
use perps_exes::{build_version, config::GasAmount, PerpsNetwork};
use perpswap::storage::MarketId;
use tracing::Level;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(clap::Parser, Clone)]
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
    /// Override the gRPC URL
    #[clap(long, env = "COSMOS_GRPC")]
    pub(crate) grpc_url: Option<String>,
    /// Provide optional gRPC fallbacks URLs
    #[clap(long, env = "COSMOS_GRPC_FALLBACKS", default_value = "")]
    pub(crate) grpc_fallbacks: GrpcFallbacks,
    /// Override the chain ID
    #[clap(long, env = "COSMOS_CHAIN_ID")]
    pub(crate) chain_id: Option<String>,
    /// Override the RPC URL
    #[clap(long, env = "COSMOS_RPC")]
    pub(crate) rpc_url: Option<String>,
    #[clap(subcommand)]
    pub(crate) sub: Sub,
    /// Override the price config file
    #[clap(long, env = "LEVANA_BOTS_PRICE_CONFIG")]
    pub(crate) price_config: Option<PathBuf>,
    /// Override chain config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_CHAIN")]
    pub(crate) config_chain: Option<PathBuf>,
    /// The stable Pyth endpoint
    #[clap(
        long,
        env = "LEVANA_BOTS_PYTH_ENDPOINT_STABLE",
        default_value = "https://hermes.pyth.network/"
    )]
    pub(crate) pyth_endpoint_stable: reqwest::Url,
    /// The edge Pyth endpoint
    #[clap(
        long,
        env = "LEVANA_BOTS_PYTH_ENDPOINT_EDGE",
        default_value = "https://hermes-beta.pyth.network/"
    )]
    pub(crate) pyth_endpoint_edge: reqwest::Url,
    /// List of markets that should be ignored
    #[clap(long, env = "LEVANA_BOTS_IGNORED_MARKETS", value_delimiter = ',')]
    pub(crate) ignored_markets: Vec<MarketId>,
    /// Reqests timeout in seconds
    #[clap(long, env = "LEVANA_BOTS_REQUEST_TIMEOUT", default_value_t = 5)]
    pub(crate) request_timeout_seconds: u64,
    /// Body length limit in bytes. Default is 1MB (Same as Nginx)
    #[clap(long, env = "LEVANA_BOTS_BODY_LIMIT", default_value_t = 1024000)]
    pub(crate) request_body_limit_bytes: usize,
    /// How many blocks we're allowed to lag before we raise an error
    #[clap(long, env = "LEVANA_BOTS_BLOCK_LAG_ALLOWED")]
    pub(crate) block_lag_allowed: Option<u32>,
    /// How many blocks we're allowed to lag before we raise an error
    #[clap(long, env = "LEVANA_BOTS_BLOCK_AGE_ALLOWED")]
    pub(crate) block_age_allowed: Option<u64>,
    /// Referer header to denote resource requesting it
    #[clap(
        long,
        env = "LEVANA_BOTS_REFERER_HEADER",
        default_value = "https://bots.levana.exchange/"
    )]
    pub(crate) referer_header: reqwest::Url,
    /// Disable optional services
    #[clap(long, env = "LEVANA_BOTS_DISABLE_OPTIONAL_SERVICES")]
    pub(crate) disable_optional_services: bool,
    /// Price bot delay in milliseconds
    #[clap(long, env = "LEVANA_BOTS_PRICE_BOT_DELAY_MILLIS")]
    pub(crate) price_bot_delay: Option<u64>,
    /// Log every query made by the system?
    #[clap(long, env = "LEVANA_BOTS_LOG_REQUESTS")]
    pub(crate) log_requests: bool,
    /// Countertrade bot
    #[clap(long, env = "LEVANA_BOTS_ENABLE_COUNTERTRADE_BOT", global = true)]
    pub(crate) enable_countertrade: bool,
    /// Enable copy trading bot
    #[clap(long, env = "LEVANA_BOTS_ENABLE_COPY_TRADE")]
    pub(crate) enable_copy_trade: bool,
    /// Is this a chain that has no gas fees?
    #[clap(long, env = "LEVANA_BOTS_NO_GAS_CHAIN")]
    pub(crate) no_gas_chain: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Parser, Clone)]
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

#[derive(clap::Parser, Clone)]
pub(crate) struct TestnetOpt {
    /// hCaptcha secret key
    #[clap(long, env = "LEVANA_BOTS_HCAPTCHA_SECRET")]
    pub(crate) hcaptcha_secret: String,
    /// Gas wallet, shared across all contract families on a chain.
    #[clap(long, env = "LEVANA_BOTS_PHRASE_GAS")]
    pub(crate) gas_phrase: SeedPhrase,
    /// How many wallets to put in the pool
    #[clap(long, env = "LEVANA_BOTS_POOL_WALLET_COUNT", default_value_t = 4)]
    pub(crate) pool_wallet_count: usize,
    /// Maintenance mode to use. Empty string is treated as no maintenance mode.
    #[clap(long, env = "LEVANA_BOTS_MAINTENANCE")]
    pub(crate) maintenance: Option<String>,
    /// Override the number of trading bots to run
    #[clap(long, env = "LEVANA_BOTS_TRADERS")]
    pub(crate) traders: Option<u32>,
    /// Deployment name to use (aka contract family)
    #[clap(long, env = "LEVANA_BOTS_DEPLOYMENT")]
    pub(crate) deployment: String,
    #[clap(long, env = "COSMOS_GAS_MULTIPLIER")]
    pub(crate) gas_multiplier: Option<f64>,
    /// Override testnet config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_TESTNET")]
    pub(crate) config_testnet: Option<PathBuf>,
    /// Number of seconds before HTTP connections (especially to Pyth) will time out
    #[clap(long, env = "LEVANA_BOTS_HTTP_TIMEOUT_SECONDS", default_value_t = 10)]
    pub(crate) http_timeout_seconds: u32,
}

#[derive(clap::Parser, Clone)]
pub(crate) struct MainnetOpt {
    #[clap(long, env = "LEVANA_BOTS_FACTORY")]
    pub(crate) factory: Address,
    #[clap(long, env = "LEVANA_BOTS_SEED_PHRASE")]
    pub(crate) seed: SeedPhrase,
    /// How many wallets to put in the pool
    #[clap(long, env = "LEVANA_BOTS_POOL_WALLET_COUNT", default_value_t = 4)]
    pub(crate) pool_wallet_count: usize,
    #[clap(long, env = "COSMOS_NETWORK")]
    pub(crate) network: PerpsNetwork,
    #[clap(long, env = "COSMOS_GAS_MULTIPLIER")]
    pub(crate) gas_multiplier: Option<f64>,
    /// Used for both price and crank wallets
    #[clap(long, env = "LEVANA_BOTS_MIN_GAS")]
    pub(crate) min_gas: GasAmount,
    /// Used for the very high gas wallet on Osmosis
    #[clap(long, env = "LEVANA_BOTS_MIN_GAS_HIGH_GAS_WALLET")]
    pub(crate) min_gas_high_gas_wallet: GasAmount,
    /// Minimum required in the refill wallet used to top off price and crank wallets
    #[clap(long, env = "LEVANA_BOTS_MIN_GAS_REFILL")]
    pub(crate) min_gas_refill: GasAmount,
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
    /// Used for RPC health checks
    #[clap(long, env = "LEVANA_BOTS_RPC_ENDPOINT")]
    pub(crate) rpc_endpoint: String,
    /// How many crank wallets to use
    #[clap(long, env = "LEVANA_BOTS_CRANK_WALLETS", default_value_t = 4)]
    pub(crate) crank_wallets: usize,
    /// How many seconds to ignore errors after an epoch
    #[clap(
        long,
        env = "LEVANA_BOTS_IGNORE_ERRORS_AFTER_EPOCH_SECONDS",
        default_value_t = 300
    )]
    pub(crate) ignore_errors_after_epoch_seconds: u32,
    /// Gas price at which we consider Osmosis to be congested
    #[clap(long, env = "LEVANA_BOTS_GAS_PRICE_CONGESTED", default_value_t = 0.004)]
    pub(crate) gas_price_congested: f64,
    /// Maximum gas price we'll pay on Osmosis
    #[clap(long, env = "LEVANA_BOTS_MAX_GAS_PRICE", default_value_t = 0.0486)]
    pub(crate) max_gas_price: f64,
    /// Maximum gas price we'll pay on Osmosis for urgent messages
    #[clap(long, env = "LEVANA_BOTS_HIGHER_MAX_GAS_PRICE", default_value_t = 0.2)]
    pub(crate) higher_max_gas_price: f64,
    /// Maximum gas price we'll pay on Osmosis for urgent messages
    #[clap(
        long,
        env = "LEVANA_BOTS_VERY_HIGHER_MAX_GAS_PRICE",
        default_value_t = 2.0
    )]
    pub(crate) very_higher_max_gas_price: f64,
}

impl Opt {
    pub(crate) fn init_logger(&self) -> Result<()> {
        let env_directive = if self.verbose {
            format!(
                "{}=debug,cosmos=debug,levana=debug,info",
                env!("CARGO_CRATE_NAME")
            )
            .parse()?
        } else {
            Level::INFO.into()
        };

        tracing_subscriber::registry()
            .with(
                fmt::Layer::default()
                    .log_internal_errors(true)
                    .and_then(EnvFilter::from_default_env().add_directive(env_directive)),
            )
            .init();
        tracing::info!("Initialized Logging");
        Ok(())
    }

    pub(crate) fn get_wallet_seed(&self, wallet_phrase_name: &str) -> Result<SeedPhrase> {
        let env_var = format!("LEVANA_BOTS_PHRASE_{wallet_phrase_name}");
        let phrase = get_env(&env_var)?;
        SeedPhrase::from_str(&phrase).map_err(|e| e.into())
    }
}

fn get_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Unable to load enviornment variable {key}"))
}

#[derive(Clone, Debug)]
pub(crate) struct GrpcFallbacks(pub(crate) Vec<String>);

impl FromStr for GrpcFallbacks {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.is_empty() {
            Ok(Self(vec![]))
        } else {
            Ok(Self(
                s.split(',')
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
            ))
        }
    }
}
