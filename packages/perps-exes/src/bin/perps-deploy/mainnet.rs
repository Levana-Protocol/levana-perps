use std::collections::HashMap;

use chrono::{DateTime, Utc};
use cosmos::{Address, CosmosNetwork, HasAddress};
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
    /// Instantiate a new factory contract
    InstantiateFactory {
        #[clap(flatten)]
        inner: InstantiateFactoryOpts,
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
        Sub::InstantiateFactory { inner } => {
            instantiate_factory(opt, inner).await?;
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

#[derive(clap::Parser)]
struct InstantiateFactoryOpts {
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: CosmosNetwork,
    /// Unique label used internally for this factory
    #[clap(long)]
    id: String,
    /// On-chain label for the factory contract
    #[clap(long)]
    factory_label: String,
    /// Label suffix applied to contracts the factory itself instantiates
    #[clap(long)]
    label_suffix: Option<String>,
    /// Owner wallet to use. Is also used as the default for any other addresses not provided.
    #[clap(long)]
    owner: Address,
    /// Migration admin wallet
    #[clap(long)]
    migration_admin: Option<Address>,
    /// DAO wallet (receives protocol fees)
    #[clap(long)]
    dao: Option<Address>,
    /// Kill switch
    #[clap(long)]
    kill_switch: Option<Address>,
    /// Market wind down
    #[clap(long)]
    wind_down: Option<Address>,
}

/// Stores mainnet factory contract IDs, keyed by a unique identifier
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct MainnetFactories {
    factories: HashMap<String, MainnetFactory>,
}

/// An instantiated factory on mainnet.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct MainnetFactory {
    address: Address,
    network: CosmosNetwork,
    instantiate_code_id: u64,
    instantiate_at: DateTime<Utc>,
    gitrev: String,
    hash: String,
}

impl MainnetFactories {
    const PATH: &str = "packages/perps-exes/assets/mainnet-factories.yaml";

    fn load() -> Result<Self> {
        let mut file = fs_err::File::open(Self::PATH)?;
        serde_yaml::from_reader(&mut file)
            .with_context(|| format!("Error loading MainnetFactories from {}", Self::PATH))
    }

    fn save(&self) -> Result<()> {
        let mut file = fs_err::File::create(Self::PATH)?;
        serde_yaml::to_writer(&mut file, self)
            .with_context(|| format!("Error saving MainnetFactories to {}", Self::PATH))
    }
}

async fn instantiate_factory(
    opt: Opt,
    InstantiateFactoryOpts {
        network,
        id,
        factory_label,
        label_suffix,
        owner,
        migration_admin,
        dao,
        kill_switch,
        wind_down,
    }: InstantiateFactoryOpts,
) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;
    let code_ids = CodeIds::load()?;
    let mut factories = MainnetFactories::load()?;

    anyhow::ensure!(
        !factories.factories.contains_key(&id),
        "Factory ID already in use: {id}"
    );

    let get_code_id = |contract_type: ContractType| {
        let contract_path = opt.get_contract_path(contract_type.as_str());
        let hash = get_hash_for_path(&contract_path)?;
        let stored_contract = code_ids.hashes.get(&hash).with_context(|| {
            format!("Mainnet code IDs list does not include hash {hash}, do you need to store?")
        })?;
        anyhow::ensure!(stored_contract.contract_type == contract_type, "Mismatched contract type for SHA256 {hash}. Expected: {contract_type:?}. Found in file: {:?}", stored_contract.contract_type);
        let code_id = stored_contract.code_ids.get(&network).with_context(|| format!("Mainnet code IDs list does not include hash {hash} on network {network}, do you need to store?")).copied()?;
        anyhow::Ok((&stored_contract.gitrev, hash, code_id))
    };

    let (gitrev, hash, factory_code_id) = get_code_id(ContractType::Factory)?;
    let (_, _, market) = get_code_id(ContractType::Market)?;
    let (_, _, position) = get_code_id(ContractType::PositionToken)?;
    let (_, _, liquidity) = get_code_id(ContractType::LiquidityToken)?;
    let factory = app.cosmos.make_code_id(factory_code_id);
    let factory = factory
        .instantiate(
            &app.wallet,
            factory_label,
            vec![],
            msg::contracts::factory::entry::InstantiateMsg {
                market_code_id: market.to_string(),
                position_token_code_id: position.to_string(),
                liquidity_token_code_id: liquidity.to_string(),
                owner: owner.get_address_string().into(),
                migration_admin: migration_admin.unwrap_or(owner).get_address_string().into(),
                dao: dao.unwrap_or(owner).get_address_string().into(),
                kill_switch: kill_switch.unwrap_or(owner).get_address_string().into(),
                wind_down: wind_down.unwrap_or(owner).get_address_string().into(),
                label_suffix,
            },
        )
        .await?;
    log::info!("Deployed fresh factory contract to: {factory}");

    factories.factories.insert(
        id,
        MainnetFactory {
            address: factory.get_address(),
            network: network,
            instantiate_code_id: factory_code_id,
            instantiate_at: Utc::now(),
            gitrev: gitrev.clone(),
            hash,
        },
    );
    factories.save()?;

    Ok(())
}
