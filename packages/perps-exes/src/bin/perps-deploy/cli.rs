use std::path::PathBuf;

use anyhow::Result;
use cosmos::RawWallet;
use perps_exes::build_version;

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
    /// Instantiate rewards contracts
    InstantiateRewards {
        #[clap(flatten)]
        inner: crate::instantiate_rewards::InstantiateRewardsOpt,
    },
    /// Migrate existing contracts
    Migrate {
        #[clap(flatten)]
        inner: crate::migrate::MigrateOpt,
    },
    /// Migrate rewards contracts
    MigrateRewards {
        #[clap(flatten)]
        inner: crate::migrate_rewards::MigrateRewardsOpt,
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
}

#[derive(clap::Parser)]
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
}

#[derive(clap::Parser, Clone)]
pub(crate) struct Opt {
    /// Override gRPC endpoint
    #[clap(long, env = "COSMOS_GRPC", global = true)]
    pub(crate) cosmos_grpc: Option<String>,
    /// Use RPC instead of gRPC
    #[clap(long, env = "COSMOS_RPC", global = true)]
    pub(crate) cosmos_rpc: Option<String>,
    /// Override chain ID
    #[clap(long, env = "COSMOS_CHAIN_ID", global = true)]
    pub(crate) cosmos_chain_id: Option<String>,
    /// Mnemonic phrase for the Wallet
    #[clap(long, env = "COSMOS_WALLET")]
    pub(crate) wallet: Option<RawWallet>,
    /// Turn on verbose logging
    #[clap(long, short, global = true)]
    verbose: bool,
    /// Directory containing the generated WASM files.
    #[clap(long, default_value = "./wasm/artifacts", env = "PERPS_WASM_DIR")]
    wasm_dir: PathBuf,
}

impl Opt {
    pub(crate) fn init_logger(&self) {
        let env = env_logger::Env::default().default_filter_or(if self.verbose {
            format!("{}=debug,cosmos=debug,info", env!("CARGO_CRATE_NAME"))
        } else {
            "info".to_owned()
        });
        env_logger::Builder::from_env(env).init();
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
