use std::path::PathBuf;

use anyhow::Result;
use cosmos::SeedPhrase;
use perps_exes::build_version;
use reqwest::Url;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Layer};

use crate::localtest;

#[derive(clap::Parser)]
#[clap(version = build_version())]
pub(crate) struct Cmd {
    #[clap(flatten)]
    pub opt: Opt,
    #[clap(subcommand)]
    pub(crate) subcommand: Subcommand,
}

#[derive(clap::Parser)]
pub(crate) enum TestnetSub {
    /// Store the contracts and notify the tracker. Skips any contracts that are
    /// already uploaded.
    StoreCode {
        #[clap(flatten)]
        inner: crate::store_code::StoreCodeOpt,
    },
    /// Instantiate perps contracts
    Instantiate {
        #[clap(flatten)]
        inner: crate::instantiate::InstantiateOpt,
    },
    /// Add a market to an existing contract
    AddMarket {
        #[clap(flatten)]
        inner: crate::testnet::add_market::AddMarketOpt,
    },
    /// Migrate existing contracts
    Migrate {
        #[clap(flatten)]
        inner: crate::migrate::MigrateOpt,
    },
    /// Instantiate chain-wide contracts as a one time setup
    InitChain {
        #[clap(flatten)]
        inner: crate::init_chain::InitChainOpt,
    },
    /// Deposit collateral into a market, useful for the trading competition
    Deposit {
        #[clap(flatten)]
        inner: crate::testnet::deposit::DepositOpt,
    },
    /// Enable a market, useful for starting a trading competition
    EnableMarket {
        #[clap(flatten)]
        inner: crate::testnet::enable_market::EnableMarketOpt,
    },
    /// Disable a market at the given timestamp, good for trading competition
    DisableMarketAt {
        #[clap(flatten)]
        inner: crate::testnet::disable::DisableMarketAtOpt,
    },
    /// Close all positions in the market, good for trading competition
    CloseAllPositions {
        #[clap(flatten)]
        inner: crate::testnet::disable::CloseAllPositionsOpt,
    },
    /// Update the configs in all markets with the given message
    UpdateMarketConfigs {
        #[clap(flatten)]
        inner: crate::testnet::update_market_configs::UpdateMarketConfigsOpt,
    },
    /// Sync config against the local config
    SyncConfig {
        #[clap(flatten)]
        inner: crate::testnet::sync_config::SyncConfigOpts,
    },
}

#[derive(clap::Parser)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Subcommand {
    /// Do a complete local deployment
    LocalDeploy {
        #[clap(flatten)]
        inner: crate::local_deploy::LocalDeployOpt,
    },
    /// On Chain tests
    OnChainTests {
        #[clap(flatten)]
        inner: localtest::TestsOpt,
    },
    /// Testnet-specific commands.
    Testnet {
        #[clap(subcommand)]
        inner: TestnetSub,
    },
    /// Mainnet-specific deployment activities.
    Mainnet {
        #[clap(flatten)]
        inner: crate::mainnet::MainnetOpt,
    },
    /// General purpose utility commands
    Util {
        #[clap(flatten)]
        inner: crate::util_cmd::UtilOpt,
    },
}

#[derive(clap::Parser, Clone)]
pub(crate) struct Opt {
    /// Override gRPC endpoint
    #[clap(long, env = "COSMOS_GRPC", global = true)]
    pub(crate) cosmos_grpc: Option<Url>,
    /// Override gas multiplier
    #[clap(long, env = "COSMOS_GAS_MULTIPLIER", global = true)]
    pub(crate) cosmos_gas_multiplier: Option<f64>,
    /// Override chain ID
    #[clap(long, env = "COSMOS_CHAIN_ID", global = true)]
    pub(crate) cosmos_chain_id: Option<String>,
    /// Mnemonic phrase for the Wallet
    #[clap(long, env = "COSMOS_WALLET")]
    pub(crate) wallet: Option<SeedPhrase>,
    /// Gas coin (e.g. uosmo)
    #[clap(long, global = true, env = "COSMOS_GAS_COIN")]
    pub(crate) cosmos_gas_coin: Option<String>,
    /// Turn on verbose logging
    #[clap(long, short, global = true)]
    verbose: bool,
    /// Directory containing the generated WASM files.
    #[clap(long, default_value = "./wasm/artifacts", env = "PERPS_WASM_DIR")]
    wasm_dir: PathBuf,
    /// Override Price config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_PRICE")]
    pub(crate) config_price: Option<PathBuf>,
    /// Override chain config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_CHAIN")]
    pub(crate) config_chain: Option<PathBuf>,
    /// Override testnet config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_TESTNET")]
    pub(crate) config_testnet: Option<PathBuf>,
    /// Override the market config update file
    #[clap(
        long,
        env = "LEVANA_BOTS_MARKET_CONFIG_UPDATE",
        default_value = "packages/perps-exes/assets/market-config-updates.toml"
    )]
    pub(crate) market_config: PathBuf,
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
}

impl Opt {
    pub(crate) fn init_logger(&self) -> anyhow::Result<()> {
        let env_filter = EnvFilter::from_default_env();

        let crate_name = env!("CARGO_CRATE_NAME");
        let env_filter = match std::env::var("RUST_LOG") {
            Ok(_) => env_filter,
            Err(_) => {
                if self.verbose {
                    env_filter
                        .add_directive("cosmos=debug".parse()?)
                        .add_directive(format!("{}=debug", crate_name).parse()?)
                } else {
                    env_filter
                        .add_directive(format!("{}=info", crate_name).parse()?)
                        .add_directive("tower_http=info".parse()?)
                }
            }
        };

        tracing_subscriber::registry()
            .with(
                fmt::Layer::default()
                    .with_writer(std::io::stderr)
                    .log_internal_errors(true)
                    .and_then(env_filter),
            )
            .init();

        tracing::debug!("Debug message!");
        Ok(())
    }

    /// Get the gitrev from the gitrev file in the wasmdir
    pub(crate) fn get_gitrev(&self) -> Result<String> {
        let mut path = self.wasm_dir.to_owned();
        path.push("gitrev");
        Ok(String::from_utf8(fs_err::read(&path)?)?
            .chars()
            .take_while(|c| !c.is_ascii_whitespace())
            .collect::<String>())
    }

    /// Get the file path of a specific contract type
    pub(crate) fn get_contract_path(&self, contract_type: &str) -> PathBuf {
        let mut path = self.wasm_dir.to_owned();
        path.push(format!("levana_perpswap_cosmos_{contract_type}.wasm"));
        path
    }
}
