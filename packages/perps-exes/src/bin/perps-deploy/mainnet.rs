use std::collections::HashMap;

use cosmos::CosmosNetwork;
use perps_exes::prelude::*;

use crate::{cli::Opt, init_chain::TRACKER, util::get_hash_for_path};

#[derive(clap::Parser)]
pub(crate) struct MainnetOpt {
    #[clap(subcommand)]
    sub: Sub,
}

#[derive(clap::Parser)]
enum Sub {
    /// Store the tracker code on chain
    StoreTracker {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: CosmosNetwork,
    },
    /// Instantiate a new tracker
    InstantiateTracker {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: CosmosNetwork,
        /// Tracker code ID
        #[clap(long)]
        tracker_code_id: u64,
    },
    /// Migrate an existing tracker to a newer code version
    MigrateTracker {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: CosmosNetwork,
        /// Tracker code ID
        #[clap(long)]
        tracker_code_id: u64,
    },
    /// Store all perps contracts on chain
    StorePerpsContracts {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: CosmosNetwork,
    },
}

pub(crate) async fn go(opt: Opt, inner: MainnetOpt) -> Result<()> {
    match inner.sub {
        Sub::StoreTracker { network } => store_tracker(opt, network).await?,
        Sub::InstantiateTracker {
            network,
            tracker_code_id,
        } => instantiate_tracker(opt, network, tracker_code_id).await?,
        Sub::MigrateTracker {
            network,
            tracker_code_id,
        } => migrate_tracker(opt, network, tracker_code_id).await?,
        Sub::StorePerpsContracts { network } => {
            store_perps_contracts(opt, network).await?;
        }
    }
    Ok(())
}

async fn store_tracker(opt: Opt, network: CosmosNetwork) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;

    log::info!("Storing tracker code...");
    let tracker_code_id = app
        .cosmos
        .store_code_path(&app.wallet, opt.get_contract_path(TRACKER))
        .await?;

    log::info!("New tracker code ID for network {network} is {tracker_code_id}");

    Ok(())
}

async fn instantiate_tracker(opt: Opt, network: CosmosNetwork, tracker_code_id: u64) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;
    let contract = app
        .cosmos
        .make_code_id(tracker_code_id)
        .instantiate(
            &app.wallet,
            "Levana Contract Tracker",
            vec![],
            msg::contracts::tracker::entry::InstantiateMsg {},
        )
        .await?;
    log::info!("New tracker contract is: {contract}");
    log::info!("Please store in the config-chain.yaml file");
    Ok(())
}

async fn migrate_tracker(opt: Opt, network: CosmosNetwork, tracker_code_id: u64) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;
    let tracker = app
        .tracker
        .with_context(|| format!("No tracker found for network {network}"))?;
    tracker
        .0
        .migrate(
            &app.wallet,
            tracker_code_id,
            msg::contracts::tracker::entry::MigrateMsg {},
        )
        .await?;
    log::info!("Tracker contract {} is migrated", tracker.0);
    Ok(())
}

/// Stores code ID by the SHA256 hash of the contract.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct CodeIds {
    hashes: HashMap<String, StoredContract>,
}

impl CodeIds {
    const PATH: &str = "packages/perps-exes/assets/mainnet-code-ids.yaml";

    fn load() -> Result<Self> {
        let mut file = fs_err::File::open(Self::PATH)?;
        serde_yaml::from_reader(&mut file)
            .with_context(|| format!("Error loading CodeIds from {}", Self::PATH))
    }

    fn save(&self) -> Result<()> {
        let mut file = fs_err::File::create(Self::PATH)?;
        serde_yaml::to_writer(&mut file, self)
            .with_context(|| format!("Error saving CodeIds to {}", Self::PATH))
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
enum ContractType {
    Factory,
    Market,
    LiquidityToken,
    PositionToken,
    PythBridge,
}

impl ContractType {
    fn all() -> [ContractType; 5] {
        use ContractType::*;
        [Factory, Market, LiquidityToken, PositionToken, PythBridge]
    }

    fn as_str(self) -> &'static str {
        match self {
            ContractType::Factory => "factory",
            ContractType::Market => "market",
            ContractType::LiquidityToken => "liquidity_token",
            ContractType::PositionToken => "position_token",
            ContractType::PythBridge => "pyth_bridge",
        }
    }
}

/// Information about a single compiled contract.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct StoredContract {
    contract_type: ContractType,
    gitrev: String,
    code_ids: HashMap<CosmosNetwork, u64>,
}

async fn store_perps_contracts(opt: Opt, network: CosmosNetwork) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;
    let mut code_ids = CodeIds::load()?;
    let gitrev = opt.get_gitrev()?;

    for contract_type in ContractType::all() {
        let contract_path = opt.get_contract_path(contract_type.as_str());
        let hash = get_hash_for_path(&contract_path)?;
        let entry = code_ids
            .hashes
            .entry(hash.clone())
            .or_insert_with(|| StoredContract {
                contract_type,
                gitrev: gitrev.clone(),
                code_ids: HashMap::new(),
            });
        anyhow::ensure!(entry.contract_type == contract_type, "Mismatched contract type for SHA256 {hash}. Expected: {contract_type:?}. Found in file: {:?}", entry.contract_type);
        match entry.code_ids.get(&network) {
            Some(code_id) => {
                log::info!("{contract_type:?} already found under code ID {code_id}");
            }
            None => {
                log::info!("Storing {contract_type:?}...");
                let code_id = app
                    .cosmos
                    .store_code_path(&app.wallet, &contract_path)
                    .await?;
                log::info!("New code ID: {code_id}");
                entry.code_ids.insert(network, code_id.get_code_id());
                code_ids.save()?;
            }
        }
    }

    Ok(())
}
