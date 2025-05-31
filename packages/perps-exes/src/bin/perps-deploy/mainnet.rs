mod check_price_feed_health;
mod close_all_positions;
mod contracts_csv;
mod fees_paid;
mod migrate;
mod rewards;
mod send_treasury;
mod sync_config;
mod transfer_dao_fees;
mod update_config;
mod wind_down;

use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use cosmos::{Address, ContractAdmin, Cosmos, HasAddress, TxBuilder};
use cosmwasm_std::{to_json_binary, CosmosMsg, Empty};
use perps_exes::{
    config::{
        load_toml, save_toml, ChainConfig, ConfigUpdateAndBorrowFee, CrankFeeConfig,
        MainnetFactories, MainnetFactory, MarketConfigUpdates, PriceConfig,
    },
    contracts::Factory,
    prelude::*,
    PerpsNetwork,
};
use perpswap::contracts::market::{
    entry::NewMarketParams,
    spot_price::{SpotPriceConfigInit, SpotPriceFeedDataInit},
};

use crate::{cli::Opt, spot_price_config::get_spot_price_config, util::get_hash_for_path};

use self::{
    check_price_feed_health::CheckPriceFeedHealthOpts, close_all_positions::CloseAllPositionsOpts,
    contracts_csv::ContractsCsvOpts, migrate::MigrateOpts, send_treasury::SendTreasuryOpts,
    sync_config::SyncConfigOpts, transfer_dao_fees::TransferDaoFeesOpts,
    update_config::UpdateConfigOpts, wind_down::WindDownOpts,
};

#[derive(clap::Parser)]
pub(crate) struct MainnetOpt {
    #[clap(subcommand)]
    sub: Sub,
}

#[derive(clap::Parser)]
#[allow(clippy::large_enum_variant)]
enum Sub {
    /// Store all perps contracts on chain
    StorePerpsContracts {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: PerpsNetwork,
        /// Override the code ID, only works if to-upload is specified and has a single value
        #[clap(long)]
        code_id: Option<u64>,
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
    /// Synchronize on-chain config with market config updates file
    SyncConfig {
        #[clap(flatten)]
        inner: SyncConfigOpts,
    },
    /// Create a CW3 message to send funds from the treasury wallet
    SendTreasury {
        #[clap(flatten)]
        inner: SendTreasuryOpts,
    },
    /// Transfer accumulated fees from the markets to the dao treasury
    TransferDaoFees {
        #[clap(flatten)]
        inner: TransferDaoFeesOpts,
    },
    /// Create a CW3 message to perform a market wind down operation
    WindDown {
        #[clap(flatten)]
        inner: WindDownOpts,
    },
    /// Exports all contract addresses for a factory to a CSV file
    ContractsCsv {
        #[clap(flatten)]
        inner: ContractsCsvOpts,
    },
    /// Check the health of all the price feeds for a factory
    CheckPriceFeedHealth {
        #[clap(flatten)]
        inner: CheckPriceFeedHealthOpts,
    },
    /// Create a CW3 message to close all positions in a market
    CloseAllPositions {
        #[clap(flatten)]
        inner: CloseAllPositionsOpts,
    },
    /// Collect rewards in all markets in a mainnet factory
    Rewards {
        #[clap(flatten)]
        inner: rewards::RewardsOpts,
    },
    /// Produce a report on fees paid by a wallet across a factory
    FeesPaid {
        #[clap(flatten)]
        inner: fees_paid::FeesPaidOpts,
    },
}

pub(crate) async fn go(opt: Opt, inner: MainnetOpt) -> Result<()> {
    match inner.sub {
        Sub::StorePerpsContracts {
            network,
            code_id,
            granter,
            to_upload,
        } => {
            store_perps_contracts(opt, network, code_id, granter, &to_upload).await?;
        }
        Sub::InstantiateFactory { inner } => {
            instantiate_factory(opt, inner).await?;
        }
        Sub::AddMarket { inner } => add_market(opt, inner).await?,
        Sub::Migrate { inner } => inner.go(opt).await?,
        Sub::UpdateConfig { inner } => inner.go(opt).await?,
        Sub::SyncConfig { inner } => inner.go(opt).await?,
        Sub::SendTreasury { inner } => inner.go(opt).await?,
        Sub::TransferDaoFees { inner } => inner.go(opt).await?,
        Sub::WindDown { inner } => inner.go(opt).await?,
        Sub::ContractsCsv { inner } => inner.go(opt).await?,
        Sub::CheckPriceFeedHealth { inner } => inner.go(opt).await?,
        Sub::CloseAllPositions { inner } => inner.go(opt).await?,
        Sub::Rewards { inner } => inner.go(opt).await?,
        Sub::FeesPaid { inner } => inner.go(opt).await?,
    }
    Ok(())
}

/// Stores code ID by the SHA256 hash of the contract.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct CodeIds {
    /// Uses a Vec instead of a HashMap to keep consistent ordering and avoid large diffs.
    hashes: Vec<StoredContract>,
}

impl CodeIds {
    const PATH: &'static str = "packages/perps-exes/assets/mainnet-code-ids.toml";

    pub(crate) fn load() -> Result<Self> {
        load_toml(Self::PATH, "LEVANA_CODE_IDS_", "code IDs")
    }

    fn save(&self) -> Result<()> {
        save_toml(Self::PATH, self)
    }

    fn get_mut_by_hash(&mut self, hash: &str) -> Option<&mut StoredContract> {
        self.hashes.iter_mut().find(|x| x.hash == hash)
    }

    fn get(
        &self,
        contract_type: ContractType,
        opt: &Opt,
        network: PerpsNetwork,
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
        network: PerpsNetwork,
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
    Countertrade,
    Vault,
}

impl ContractType {
    fn all_required() -> [ContractType; 6] {
        use ContractType::*;
        [
            Factory,
            Market,
            LiquidityToken,
            PositionToken,
            Countertrade,
            Vault,
        ]
    }

    fn as_str(self) -> &'static str {
        match self {
            ContractType::Factory => "factory",
            ContractType::Market => "market",
            ContractType::LiquidityToken => "liquidity_token",
            ContractType::PositionToken => "position_token",
            ContractType::Countertrade => "countertrade",
            ContractType::Vault => "vault",
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
            "countertrade" => Ok(ContractType::Countertrade),
            "vault" => Ok(ContractType::Vault),
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
    code_ids: BTreeMap<PerpsNetwork, u64>,
    hash: String,
}

async fn store_perps_contracts(
    opt: Opt,
    network: PerpsNetwork,
    code_id: Option<u64>,
    granter: Option<Address>,
    to_upload: &[ContractType],
) -> Result<()> {
    let app = opt.load_app_mainnet(network).await?;
    let wallet = app.get_wallet()?;
    let mut code_ids = CodeIds::load()?;
    let gitrev = opt.get_gitrev()?;

    let all_contracts = ContractType::all_required();
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
                    code_ids: BTreeMap::new(),
                    hash: hash.clone(),
                });
                code_ids.hashes.last_mut().expect("last cannot be null")
            }
        };
        anyhow::ensure!(entry.contract_type == contract_type, "Mismatched contract type for SHA256 {hash}. Expected: {contract_type:?}. Found in file: {:?}", entry.contract_type);
        match entry.code_ids.get(&network) {
            Some(code_id) => {
                tracing::info!("{contract_type:?} already found under code ID {code_id}");
            }
            None => {
                let code_id = match code_id {
                    Some(code_id) => {
                        anyhow::ensure!(
                            to_upload.len() == 1,
                            "Can only provide a code ID if there is exactly one to-upload value"
                        );
                        tracing::info!("Using code ID from the command line: {code_id}");
                        code_id
                    }
                    None => {
                        tracing::info!("Storing {contract_type:?}...");
                        let code_id = match granter {
                            None => app.cosmos.store_code_path(wallet, &contract_path).await?,
                            Some(granter) => {
                                app.cosmos
                                    .store_code_path_authz(wallet, &contract_path, granter)
                                    .await?
                                    .1
                            }
                        };
                        tracing::info!("New code ID: {code_id}");
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
    network: PerpsNetwork,
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
    let wallet = app.get_wallet()?;
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
    tracing::info!("Instantiating a factory using code ID {factory_code_id}");
    let migration_admin = migration_admin.unwrap_or(owner);
    let factory = factory
        .instantiate(
            wallet,
            factory_label.clone(),
            vec![],
            perpswap::contracts::factory::entry::InstantiateMsg {
                market_code_id: market.to_string(),
                position_token_code_id: position.to_string(),
                liquidity_token_code_id: liquidity.to_string(),
                owner: owner.get_address_string().into(),
                migration_admin: migration_admin.get_address_string().into(),
                dao: dao.unwrap_or(owner).get_address_string().into(),
                kill_switch: kill_switch.unwrap_or(owner).get_address_string().into(),
                wind_down: wind_down.unwrap_or(owner).get_address_string().into(),
                label_suffix,
                copy_trading_code_id: None,
                counter_trade_code_id: None,
            },
            ContractAdmin::Addr(migration_admin),
        )
        .await?;
    tracing::info!("Deployed fresh factory contract to: {factory}");

    factories.factories.push(MainnetFactory {
        address: factory.get_address(),
        network,
        label: factory_label,
        instantiate_code_id: factory_code_id,
        instantiate_at: Utc::now(),
        gitrev: gitrev.to_owned(),
        hash,
        ident: Some(ident),
        canonical: false,
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
    #[clap(long, required = true)]
    market: Vec<MarketId>,
}

async fn add_market(opt: Opt, AddMarketOpts { factory, market }: AddMarketOpts) -> Result<()> {
    let market_config_updates = MarketConfigUpdates::load(&opt.market_config)?;

    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;
    let chain_config = ChainConfig::load(factory.network)?;
    let price_config = PriceConfig::load()?;
    let oracle = opt.get_oracle_info(&chain_config, &price_config, factory.network)?;

    let mut simtx = TxBuilder::default();
    let mut msgs = vec![];

    let network = factory.network;
    let factory = app.cosmos.make_contract(factory.address);
    let factory = Factory::from_contract(factory);
    let owner = factory
        .query_owner()
        .await?
        .context("The factory owner is not provided")?;

    for market_id in market {
        let ConfigUpdateAndBorrowFee {
            config: mut market_config_update,
            initial_borrow_fee_rate,
        } = {
            market_config_updates
                .markets
                .get(&market_id)
                .cloned()
                .with_context(|| format!("No config update found for market ID: {market_id}"))?
        };
        let CrankFeeConfig {
            charged,
            surcharge,
            reward,
        } = market_config_updates
            .crank_fees
            .get(&network)
            .with_context(|| format!("No crank fee config found for network {network}"))?;
        market_config_update.crank_fee_charged = Some(*charged);
        market_config_update.crank_fee_surcharge = Some(*surcharge);
        market_config_update.crank_fee_reward = Some(*reward);

        let collateral_name = market_id.get_collateral();
        let token = chain_config
            .assets
            .get(collateral_name)
            .with_context(|| {
                format!("No definition for asset {collateral_name} for network {network}",)
            })?
            .into();

        let spot_price = get_spot_price_config(&oracle, &market_id)?;
        validate_spot_price_config(&app.cosmos, &spot_price, &market_id).await?;

        let msg = perpswap::contracts::factory::entry::ExecuteMsg::AddMarket {
            new_market: NewMarketParams {
                spot_price,
                market_id,
                token,
                config: Some(market_config_update),
                initial_borrow_fee_rate,
                initial_price: None,
            },
        };
        let msg = strip_nulls(msg)?;

        simtx.add_execute_message(&factory, owner, vec![], &msg)?;
        msgs.push(CosmosMsg::<Empty>::Wasm(cosmwasm_std::WasmMsg::Execute {
            contract_addr: factory.to_string(),
            msg: to_json_binary(&msg)?,
            funds: vec![],
        }));
    }

    tracing::info!("Need to make a proposal");

    tracing::info!("CW3 contract: {owner}");
    tracing::info!("Message: {}", serde_json::to_string(&msgs)?);

    let simres = simtx
        .simulate(&app.cosmos, &[owner])
        .await
        .context("Could not simulate message")?;
    tracing::info!("Simulation completed successfully");
    tracing::debug!("Simulation response: {simres:?}");

    Ok(())
}

async fn validate_spot_price_config(
    cosmos: &Cosmos,
    spot_price: &SpotPriceConfigInit,
    market_id: &MarketId,
) -> Result<()> {
    tracing::info!("Validating spot price config for {market_id}");

    match spot_price {
        SpotPriceConfigInit::Manual { .. } => {
            anyhow::bail!("Unsupported manual price config for {market_id}")
        }
        SpotPriceConfigInit::Oracle {
            pyth: _,
            stride,
            feeds,
            feeds_usd,
            volatile_diff_seconds: _,
        } => {
            for feed in feeds.iter().chain(feeds_usd.iter()) {
                match &feed.data {
                    // No need to check the constant feed
                    SpotPriceFeedDataInit::Constant { price: _ } => (),
                    SpotPriceFeedDataInit::Pyth {
                        id: _,
                        age_tolerance_seconds: _,
                    } => {
                        // In theory could do some sanity checking of the Pyth
                        // feeds here, but that's usually well handled via testnet testing. Skipping for
                        // now.
                    }
                    SpotPriceFeedDataInit::Stride {
                        denom,
                        age_tolerance_seconds,
                    } => {
                        #[derive(serde::Serialize)]
                        #[serde(rename_all = "snake_case")]
                        enum StrideQuery<'a> {
                            RedemptionRate { denom: &'a str },
                        }

                        let stride = stride.as_ref().with_context(|| format!("Using a Stride feed with denom {denom}, but no Stride contract configured"))?;
                        let stride =
                            cosmos.make_contract(stride.contract_address.as_str().parse()?);

                        #[derive(serde::Deserialize)]
                        #[serde(rename_all = "snake_case")]
                        struct RedemptionRateResp {
                            redemption_rate: Decimal256,
                            update_time: u64,
                        }
                        let RedemptionRateResp {
                            redemption_rate,
                            update_time,
                        } = stride.query(StrideQuery::RedemptionRate { denom }).await?;
                        let update_time = Utc
                            .timestamp_opt(update_time.try_into()?, 0)
                            .single()
                            .with_context(|| {
                                format!("Could not convert {update_time} to DateTime<Utc>")
                            })?;
                        let age = Utc::now().signed_duration_since(update_time);
                        tracing::info!(
                            "Queried Stride contract {stride} with denom {denom}, got redemption rate of {redemption_rate} updated {update_time} (age: {age:?})"
                        );
                        anyhow::ensure!(redemption_rate >= Decimal256::one(), "Redemption rates should always be at least 1, very likely the contract has the purchase rate instead. See: https://blog.levana.finance/milktia-market-mispricing-proposed-solution-6a994e9ecdfa");
                        let tolerance = chrono::Duration::seconds((*age_tolerance_seconds).into());
                        anyhow::ensure!(age < tolerance, "Stride update is too old. Expected age is less than {tolerance:?}, but got {age:?}");
                    }
                    SpotPriceFeedDataInit::Sei { denom } => anyhow::bail!(
                        "No longer supporting Sei native oracle, provided denom is: {denom}"
                    ),
                    SpotPriceFeedDataInit::Rujira { asset: _ } => (),
                    SpotPriceFeedDataInit::Simple {
                        contract,
                        age_tolerance_seconds: _,
                    } => {
                        #[derive(serde::Serialize)]
                        #[serde(rename_all = "snake_case")]
                        enum SimpleQuery {
                            Price {},
                        }
                        let contract = cosmos.make_contract(contract.as_str().parse()?);
                        let res: serde_json::Value = contract.query(SimpleQuery::Price {}).await?;
                        tracing::info!(
                            "Queried simple contract {contract}, got result {}",
                            serde_json::to_string(&res)?
                        );
                    }
                }
            }
            Ok(())
        }
    }
}

pub(crate) fn strip_nulls<T: serde::Serialize>(x: T) -> Result<serde_json::Value> {
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
