use std::collections::HashMap;

use chrono::{DateTime, Utc};
use cosmos::{Address, CosmosNetwork, HasAddress};
use msg::{
    contracts::market::{config::ConfigUpdate, entry::NewMarketParams},
    token::TokenInit,
};
use perps_exes::{config::PythConfig, prelude::*};

use crate::{cli::Opt, factory::Factory, util::get_hash_for_path};

#[derive(clap::Parser)]
pub(crate) struct MainnetOpt {
    #[clap(subcommand)]
    sub: Sub,
}

#[derive(clap::Parser)]
enum Sub {
    /// Store all perps contracts on chain
    StorePerpsContracts {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: CosmosNetwork,
        /// Override the market code ID
        #[clap(long)]
        market_code_id: Option<u64>,
    },
    /// Instantiate a new factory contract
    InstantiateFactory {
        #[clap(flatten)]
        inner: InstantiateFactoryOpts,
    },
    /// Add a new market to an existing factory
    AddMarket {
        #[clap(flatten)]
        inner: AddMarketOpts,
    },
    /// Set the Pyth price feeds for an already deployed market, mostly useful if a prior add-market failed to do this
    SetPythFeeds {
        #[clap(flatten)]
        inner: SetPythFeedsOpts,
    },
}

pub(crate) async fn go(opt: Opt, inner: MainnetOpt) -> Result<()> {
    match inner.sub {
        Sub::StorePerpsContracts {
            network,
            market_code_id,
        } => {
            store_perps_contracts(opt, network, market_code_id).await?;
        }
        Sub::InstantiateFactory { inner } => {
            instantiate_factory(opt, inner).await?;
        }
        Sub::AddMarket { inner } => add_market(opt, inner).await?,
        Sub::SetPythFeeds { inner } => set_pyth_feeds(opt, inner).await?,
    }
    Ok(())
}

/// Stores code ID by the SHA256 hash of the contract.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct CodeIds {
    /// Uses a Vec instead of a HashMap to keep consistent ordering and avoid large diffs.
    hashes: Vec<StoredContract>,
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

    fn get_mut_by_hash(&mut self, hash: &str) -> Option<&mut StoredContract> {
        self.hashes.iter_mut().find(|x| x.hash == hash)
    }

    fn get(
        &self,
        contract_type: ContractType,
        opt: &Opt,
        network: CosmosNetwork,
    ) -> Result<StoredCodeId> {
        let contract_path = opt.get_contract_path(contract_type.as_str());
        let hash = get_hash_for_path(&contract_path)?;
        let stored_contract = self
            .hashes
            .iter()
            .find(|x| x.hash == hash)
            .with_context(|| {
                format!("Mainnet code IDs list does not include hash {hash}, do you need to store?")
            })?;
        anyhow::ensure!(stored_contract.contract_type == contract_type, "Mismatched contract type for SHA256 {hash}. Expected: {contract_type:?}. Found in file: {:?}", stored_contract.contract_type);
        let code_id = stored_contract.code_ids.get(&network).with_context(|| format!("Mainnet code IDs list does not include hash {hash} on network {network}, do you need to store?")).copied()?;
        anyhow::Ok(StoredCodeId {
            gitrev: &stored_contract.gitrev,
            hash,
            code_id,
        })
    }

    fn get_simple(
        &self,
        contract_type: ContractType,
        opt: &Opt,
        network: CosmosNetwork,
    ) -> Result<u64> {
        self.get(contract_type, opt, network).map(|x| x.code_id)
    }
}

struct StoredCodeId<'a> {
    gitrev: &'a str,
    hash: String,
    code_id: u64,
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct StoredContract {
    contract_type: ContractType,
    gitrev: String,
    code_ids: HashMap<CosmosNetwork, u64>,
    hash: String,
}

async fn store_perps_contracts(
    opt: Opt,
    network: CosmosNetwork,
    market_code_id: Option<u64>,
) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;
    let mut code_ids = CodeIds::load()?;
    let gitrev = opt.get_gitrev()?;

    for contract_type in ContractType::all() {
        let contract_path = opt.get_contract_path(contract_type.as_str());
        let hash = get_hash_for_path(&contract_path)?;
        let entry = match code_ids.get_mut_by_hash(&hash) {
            Some(entry) => entry,
            None => {
                code_ids.hashes.push(StoredContract {
                    contract_type,
                    gitrev: gitrev.clone(),
                    code_ids: HashMap::new(),
                    hash: hash.clone(),
                });
                code_ids.hashes.last_mut().expect("last cannot be null")
            }
        };
        anyhow::ensure!(entry.contract_type == contract_type, "Mismatched contract type for SHA256 {hash}. Expected: {contract_type:?}. Found in file: {:?}", entry.contract_type);
        match entry.code_ids.get(&network) {
            Some(code_id) => {
                log::info!("{contract_type:?} already found under code ID {code_id}");
            }
            None => {
                let code_id = match (contract_type, market_code_id) {
                    (ContractType::Market, Some(code_id)) => {
                        log::info!("Using market code ID from the command line: {code_id}");
                        code_id
                    }
                    _ => {
                        log::info!("Storing {contract_type:?}...");
                        let code_id = app
                            .cosmos
                            .store_code_path(&app.wallet, &contract_path)
                            .await?;
                        log::info!("New code ID: {code_id}");
                        code_id.get_code_id()
                    }
                };
                entry.code_ids.insert(network, code_id);
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

/// Stores mainnet factory contracts
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct MainnetFactories {
    factories: Vec<MainnetFactory>,
}

/// An instantiated factory on mainnet.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct MainnetFactory {
    address: Address,
    network: CosmosNetwork,
    label: String,
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

    let StoredCodeId {
        gitrev,
        hash,
        code_id: factory_code_id,
    } = code_ids.get(ContractType::Factory, &opt, network)?;
    let market = code_ids.get_simple(ContractType::Market, &opt, network)?;
    let position = code_ids.get_simple(ContractType::PositionToken, &opt, network)?;
    let liquidity = code_ids.get_simple(ContractType::LiquidityToken, &opt, network)?;
    let factory = app.cosmos.make_code_id(factory_code_id);
    log::info!("Instantiating a factory using code ID {factory_code_id}");
    let factory = factory
        .instantiate(
            &app.wallet,
            factory_label.clone(),
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

    factories.factories.push(MainnetFactory {
        address: factory.get_address(),
        network: network,
        label: factory_label,
        instantiate_code_id: factory_code_id,
        instantiate_at: Utc::now(),
        gitrev: gitrev.to_owned(),
        hash,
    });
    factories.save()?;

    Ok(())
}

#[derive(clap::Parser)]
struct AddMarketOpts {
    /// Address of the factory contract
    #[clap(long)]
    factory: Address,
    /// New market ID to add
    #[clap(long)]
    market_id: MarketId,
    /// Denom of the native coin used for collateral
    #[clap(long)]
    collateral: String,
    /// Initial borrow fee rate
    #[clap(long, default_value = "0.2")]
    initial_borrow_fee_rate: Decimal256,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct MarketConfigUpdates {
    markets: HashMap<MarketId, ConfigUpdate>,
}

impl MarketConfigUpdates {
    const PATH: &str = "packages/perps-exes/assets/market-config-updates.yaml";

    fn load() -> Result<Self> {
        let mut file = fs_err::File::open(Self::PATH)?;
        serde_yaml::from_reader(&mut file)
            .with_context(|| format!("Error loading MarketConfigUpdates from {}", Self::PATH))
    }
}

async fn add_market(
    opt: Opt,
    AddMarketOpts {
        factory,
        market_id,
        collateral,
        initial_borrow_fee_rate,
    }: AddMarketOpts,
) -> Result<()> {
    let market_config_update = {
        let mut market_config_updates = MarketConfigUpdates::load()?;
        market_config_updates
            .markets
            .remove(&market_id)
            .with_context(|| format!("No config update found for market ID: {market_id}"))?
    };

    let pyth_config = PythConfig::load()?
        .markets
        .get(&market_id)
        .with_context(|| format!("No Pyth config found for market {market_id}"))?;

    let code_ids = CodeIds::load()?;
    let factories = MainnetFactories::load()?;
    let factory = factories
        .factories
        .into_iter()
        .find(|x| x.address == factory)
        .with_context(|| format!("Unknown mainnet factory: {factory}"))?;
    let app = opt.load_app_mainnet(Some(factory.network), None).await?;

    log::info!("Deploying a new Pyth bridge");
    let pyth_bridge = code_ids.get_simple(ContractType::PythBridge, &opt, factory.network)?;
    let pyth_bridge = app
        .cosmos
        .make_code_id(pyth_bridge)
        .instantiate(
            &app.wallet,
            format!("{} - {market_id} Pyth bridge", factory.label),
            vec![],
            msg::contracts::pyth_bridge::entry::InstantiateMsg {
                factory: factory.address.get_address_string().into(),
                pyth: app.pyth.address.get_address_string().into(),
                update_age_tolerance_seconds: app.pyth.update_age_tolerance,
            },
        )
        .await?;
    log::info!("New Pyth bridge contract: {pyth_bridge}");

    log::info!("Calling AddMarket on the factory");
    let factory = app.cosmos.make_contract(factory.address);
    let res = factory
        .execute(
            &app.wallet,
            vec![],
            msg::contracts::factory::entry::ExecuteMsg::AddMarket {
                new_market: NewMarketParams {
                    market_id: market_id.clone(),
                    token: TokenInit::Native { denom: collateral },
                    config: Some(market_config_update),
                    price_admin: pyth_bridge.get_address_string().into(),
                    initial_borrow_fee_rate,
                },
            },
        )
        .await?;
    log::info!("New market added in transaction: {}", res.txhash);

    log::info!("Setting price feed for market {market_id} to use Pyth Oracle.");
    log::info!(
        "Main price feeds: {:?}, USD price feeds: {:?}",
        pyth_config.feeds,
        pyth_config.feeds_usd
    );
    let res = pyth_bridge
        .execute(
            &app.wallet,
            vec![],
            msg::contracts::pyth_bridge::entry::ExecuteMsg::SetMarketPriceFeeds {
                market_id,
                market_price_feeds: pyth_config.clone(),
            },
        )
        .await?;
    log::info!("Called SetMarketPriceFeeds in {}", res.txhash);

    Ok(())
}

#[derive(clap::Parser)]
struct SetPythFeedsOpts {
    /// Address of the factory contract
    #[clap(long)]
    factory: Address,
    /// New market ID to add
    #[clap(long)]
    market_id: MarketId,
}

async fn set_pyth_feeds(
    opt: Opt,
    SetPythFeedsOpts { factory, market_id }: SetPythFeedsOpts,
) -> Result<()> {
    let pyth_config = PythConfig::load()?
        .markets
        .get(&market_id)
        .with_context(|| format!("No Pyth config found for market {market_id}"))?;

    let factories = MainnetFactories::load()?;
    let factory = factories
        .factories
        .into_iter()
        .find(|x| x.address == factory)
        .with_context(|| format!("Unknown mainnet factory: {factory}"))?;
    let app = opt.load_app_mainnet(Some(factory.network), None).await?;

    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));
    let pyth_bridge = factory.get_market(market_id.clone()).await?.price_admin;
    let pyth_bridge = app.cosmos.make_contract(pyth_bridge);

    log::info!("Setting price feed for market {market_id} to use Pyth Oracle using Pyth bridge {pyth_bridge}.");
    log::info!(
        "Main price feeds: {:?}, USD price feeds: {:?}",
        pyth_config.feeds,
        pyth_config.feeds_usd
    );
    let res = pyth_bridge
        .execute(
            &app.wallet,
            vec![],
            msg::contracts::pyth_bridge::entry::ExecuteMsg::SetMarketPriceFeeds {
                market_id,
                market_price_feeds: pyth_config.clone(),
            },
        )
        .await?;
    log::info!("Called SetMarketPriceFeeds in {}", res.txhash);

    Ok(())
}
