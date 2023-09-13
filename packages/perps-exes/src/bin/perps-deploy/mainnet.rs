mod migrate;
mod update_config;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use cosmos::{Address, ContractAdmin, CosmosNetwork, HasAddress};
use cosmwasm_std::{to_binary, CosmosMsg, Empty};
use msg::{contracts::market::entry::NewMarketParams, token::TokenInit};
use perps_exes::{config::MarketConfigUpdates, prelude::*};

use crate::{cli::Opt, factory::Factory, util::get_hash_for_path};

use self::{migrate::MigrateOpts, update_config::UpdateConfigOpts};

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
        /// Contract that granted store code rights
        #[clap(long)]
        granter: Option<Address>,
        /// Contract types to upload, by default uploads all needed contracts
        #[clap(long)]
        to_upload: Vec<ContractType>,
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
    /// Generate market migrate messages to be sent via CW3
    Migrate {
        #[clap(flatten)]
        inner: MigrateOpts,
    },
    /// Get a CW3 message for updating a config value
    UpdateConfig {
        #[clap(flatten)]
        inner: UpdateConfigOpts,
    },
}

pub(crate) async fn go(opt: Opt, inner: MainnetOpt) -> Result<()> {
    match inner.sub {
        Sub::StorePerpsContracts {
            network,
            market_code_id,
            granter,
            to_upload,
        } => {
            store_perps_contracts(opt, network, market_code_id, granter, &to_upload).await?;
        }
        Sub::InstantiateFactory { inner } => {
            instantiate_factory(opt, inner).await?;
        }
        Sub::AddMarket { inner } => add_market(opt, inner).await?,
        Sub::Migrate { inner } => inner.go(opt).await?,
        Sub::UpdateConfig { inner } => inner.go(opt).await?,
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
}

impl ContractType {
    fn all() -> [ContractType; 4] {
        use ContractType::*;
        [Factory, Market, LiquidityToken, PositionToken]
    }

    fn as_str(self) -> &'static str {
        match self {
            ContractType::Factory => "factory",
            ContractType::Market => "market",
            ContractType::LiquidityToken => "liquidity_token",
            ContractType::PositionToken => "position_token",
        }
    }
}

impl FromStr for ContractType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "factory" => Ok(ContractType::Factory),
            "market" => Ok(ContractType::Market),
            "liquidity_token" => Ok(ContractType::LiquidityToken),
            "position_token" => Ok(ContractType::PositionToken),
            _ => Err(anyhow::anyhow!("Invalid contract type: {s}")),
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
    granter: Option<Address>,
    to_upload: &[ContractType],
) -> Result<()> {
    let app = opt.load_app_mainnet(network).await?;
    let mut code_ids = CodeIds::load()?;
    let gitrev = opt.get_gitrev()?;

    let all_contracts = ContractType::all();
    let to_upload = if to_upload.is_empty() {
        all_contracts.as_slice()
    } else {
        to_upload
    };

    for contract_type in to_upload.iter().copied() {
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
                        let code_id = match granter {
                            None => {
                                app.cosmos
                                    .store_code_path(&app.wallet, &contract_path)
                                    .await?
                            }
                            Some(granter) => {
                                app.cosmos
                                    .store_code_path_authz(&app.wallet, &contract_path, granter)
                                    .await?
                                    .1
                            }
                        };
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
    /// Unique identifier for this market, for ease of use only
    #[clap(long)]
    ident: String,
}

/// Stores mainnet factory contracts
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct MainnetFactories {
    pub(crate) factories: Vec<MainnetFactory>,
}

impl MainnetFactories {
    pub(crate) fn get_by_address(&self, address: Address) -> Option<&MainnetFactory> {
        self.factories.iter().find(|f| f.address == address)
    }

    pub(crate) fn get_by_ident(&self, ident: &str) -> Option<&MainnetFactory> {
        self.factories
            .iter()
            .find(|f| f.ident.as_deref() == Some(ident))
    }

    /// Gets by either address or ident
    pub(crate) fn get(&self, factory: &str) -> Result<&MainnetFactory> {
        match factory.parse().ok() {
            Some(addr) => self.get_by_address(addr),
            None => self.get_by_ident(factory),
        }
        .with_context(|| format!("Unknown factory: {factory}"))
    }
}

/// An instantiated factory on mainnet.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct MainnetFactory {
    pub(crate) address: Address,
    pub(crate) network: CosmosNetwork,
    pub(crate) label: String,
    pub(crate) instantiate_code_id: u64,
    pub(crate) instantiate_at: DateTime<Utc>,
    pub(crate) gitrev: String,
    pub(crate) hash: String,
    /// A user-friendly identifier
    pub(crate) ident: Option<String>,
}

impl MainnetFactories {
    const PATH: &str = "packages/perps-exes/assets/mainnet-factories.yaml";

    pub(crate) fn load() -> Result<Self> {
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
        ident,
    }: InstantiateFactoryOpts,
) -> Result<()> {
    let app = opt.load_app_mainnet(network).await?;
    let code_ids = CodeIds::load()?;
    let mut factories = MainnetFactories::load()?;

    anyhow::ensure!(
        factories.get(&ident).is_err(),
        "Identifier already in use: {ident}"
    );

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
    let migration_admin = migration_admin.unwrap_or(owner);
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
                migration_admin: migration_admin.get_address_string().into(),
                dao: dao.unwrap_or(owner).get_address_string().into(),
                kill_switch: kill_switch.unwrap_or(owner).get_address_string().into(),
                wind_down: wind_down.unwrap_or(owner).get_address_string().into(),
                label_suffix,
            },
            ContractAdmin::Addr(migration_admin),
        )
        .await?;
    log::info!("Deployed fresh factory contract to: {factory}");

    factories.factories.push(MainnetFactory {
        address: factory.get_address(),
        network,
        label: factory_label,
        instantiate_code_id: factory_code_id,
        instantiate_at: Utc::now(),
        gitrev: gitrev.to_owned(),
        hash,
        ident: Some(ident),
    });
    factories.save()?;

    Ok(())
}

#[derive(clap::Parser)]
struct NewPythBridgeOpts {
    /// The factory contract address or ident
    #[clap(long)]
    factory: String,
    /// Market ID
    #[clap(long)]
    market_id: MarketId,
}

#[derive(clap::Parser)]
struct AddMarketOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// New market ID to add
    #[clap(long)]
    market_id: MarketId,
    /// Denom of the native coin used for collateral
    #[clap(long)]
    collateral: String,
    /// Decimal places used by this collateral asset
    #[clap(long)]
    decimal_places: u8,
    /// Initial borrow fee rate
    #[clap(long)]
    initial_borrow_fee_rate: Decimal256,
    /// Instead of executing, print out CW3 multisig instructions
    #[clap(long)]
    cw3: bool,
}

async fn add_market(
    opt: Opt,
    AddMarketOpts {
        factory,
        market_id,
        collateral,
        decimal_places,
        initial_borrow_fee_rate,
        cw3: is_cw3,
    }: AddMarketOpts,
) -> Result<()> {
    let market_config_update = {
        let mut market_config_updates = MarketConfigUpdates::load(&opt.market_config)?;
        market_config_updates
            .markets
            .remove(&market_id)
            .with_context(|| format!("No config update found for market ID: {market_id}"))?
    };

    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;

    let msg = msg::contracts::factory::entry::ExecuteMsg::AddMarket {
        new_market: NewMarketParams {
            market_id,
            token: TokenInit::Native {
                denom: collateral,
                decimal_places,
            },
            config: Some(market_config_update),
            initial_borrow_fee_rate,
            spot_price: unimplemented!("TODO"),
        },
    };
    let msg = strip_nulls(msg)?;
    let factory = app.cosmos.make_contract(factory.address);

    if is_cw3 {
        let factory = Factory::from_contract(factory);
        log::info!("Need to make a proposal");

        let owner = factory.query_owner().await?;
        log::info!("CW3 contract: {owner}");
        log::info!(
            "Message: {}",
            serde_json::to_string(&CosmosMsg::<Empty>::Wasm(cosmwasm_std::WasmMsg::Execute {
                contract_addr: factory.to_string(),
                msg: to_binary(&msg)?,
                funds: vec![]
            }))?
        );
    } else {
        log::info!("Calling AddMarket on the factory");
        let res = factory.execute(&app.wallet, vec![], msg).await?;
        log::info!("New market added in transaction: {}", res.txhash);
    }

    Ok(())
}

fn strip_nulls<T: serde::Serialize>(x: T) -> Result<serde_json::Value> {
    use serde_json::Value;
    let value = serde_json::to_value(x)?;
    fn inner(value: Value) -> Value {
        match value {
            Value::Null => Value::Null,
            Value::Bool(x) => Value::Bool(x),
            Value::Number(x) => Value::Number(x),
            Value::String(x) => Value::String(x),
            Value::Array(x) => Value::Array(x.into_iter().map(inner).collect()),
            Value::Object(x) => Value::Object(
                x.into_iter()
                    .flat_map(|(key, value)| match value {
                        Value::Null => None,
                        value => Some((key, inner(value))),
                    })
                    .collect(),
            ),
        }
    }
    Ok(inner(value))
}
