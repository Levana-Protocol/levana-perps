use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use cosmos::{Address, Cosmos};
use msg::contracts::{
    factory::entry::{MarketInfoResponse, MarketsResp},
    market::entry::StatusResp,
    tracker::entry::{CodeIdResp, ContractResp},
};
use msg::prelude::*;
use parking_lot::RwLock;
use perps_exes::config::{AddressOverride, DeploymentConfig};

use super::status_collector::{Status, StatusCategory, StatusCollector};

const UPDATE_DELAY_SECONDS: u64 = 60;
const TOO_OLD_SECONDS: i64 = 180;

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct FactoryInfo {
    pub(crate) factory: Address,
    pub(crate) faucet: Address,
    pub(crate) updated: DateTime<Utc>,
    pub(crate) is_static: bool,
    pub(crate) cw20s: Vec<Cw20>,
    pub(crate) markets: HashMap<MarketId, Address>,
    pub(crate) gitrev: Option<String>,
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct Cw20 {
    address: Address,
    denom: String,
    decimals: u8,
}

impl StatusCollector {
    pub(super) async fn start_get_factory(
        &self,
        cosmos: Cosmos,
        deployment_config: Arc<DeploymentConfig>,
    ) -> Result<Arc<RwLock<Arc<FactoryInfo>>>> {
        make_factory_task(cosmos, deployment_config, self.clone()).await
    }
}

async fn make_factory_task(
    cosmos: Cosmos,
    config: Arc<DeploymentConfig>,
    status_collector: StatusCollector,
) -> Result<Arc<RwLock<Arc<FactoryInfo>>>> {
    match config.address_override {
        Some(AddressOverride { factory, faucet }) => {
            log::info!("Using static factory contract: {factory}");
            log::info!("Using static faucet contract: {faucet}");
            status_collector.add_status(
                StatusCategory::GetFactory,
                "static",
                Status::success(
                    format!("Static factory override set to {factory}, faucet set to {faucet}, no updates coming"),
                    None,
                ),
            );

            let (cw20s, markets) = get_tokens_markets(&cosmos, factory).await?;
            Ok(Arc::new(RwLock::new(Arc::new(FactoryInfo {
                factory,
                faucet,
                updated: Utc::now(),
                is_static: true,
                cw20s,
                markets,
                gitrev: None,
            }))))
        }
        None => {
            let factory = get_factory_info(&cosmos, &config).await?;
            log::info!("Discovered factory contract: {}", factory.factory);
            log::info!("Discovered faucet contract: {}", factory.faucet);
            let arc = Arc::new(RwLock::new(Arc::new(factory)));
            let arc_clone = arc.clone();

            status_collector.add_status_check(
                StatusCategory::GetFactory,
                "load-from-tracker",
                UPDATE_DELAY_SECONDS,
                move || update(cosmos.clone(), config.clone(), arc_clone.clone()),
            );

            Ok(arc)
        }
    }
}

async fn update(
    cosmos: Cosmos,
    config: Arc<DeploymentConfig>,
    lock: Arc<RwLock<Arc<FactoryInfo>>>,
) -> Status {
    match get_factory_info(&cosmos, &config).await {
        Ok(info) => {
            let status = Status::success(
                format!(
                    "Successfully loaded factory address {} from tracker {}",
                    info.factory, config.tracker
                ),
                Some(TOO_OLD_SECONDS),
            );
            *lock.write() = Arc::new(info);
            status
        }
        Err(e) => Status::error(format!("Unable to load factory from tracker: {e:?}")),
    }
}

async fn get_factory_info(cosmos: &Cosmos, config: &DeploymentConfig) -> Result<FactoryInfo> {
    let (factory, gitrev) = get_contract(cosmos, config, "factory").await?;
    let (cw20s, markets) = get_tokens_markets(cosmos, factory).await?;
    Ok(FactoryInfo {
        factory,
        faucet: config.faucet,
        updated: Utc::now(),
        is_static: false,
        cw20s,
        markets,
        gitrev,
    })
}

async fn get_contract(
    cosmos: &Cosmos,
    config: &DeploymentConfig,
    contract_type: &str,
) -> Result<(Address, Option<String>)> {
    let tracker = cosmos.make_contract(config.tracker);
    let (addr, code_id) = match tracker
        .query(msg::contracts::tracker::entry::QueryMsg::ContractByFamily {
            contract_type: contract_type.to_owned(),
            family: config.contract_family.clone(),
            sequence: None,
        })
        .await?
    {
        ContractResp::NotFound {} => anyhow::bail!(
            "No {contract_type} contract found for contract family {}",
            config.contract_family
        ),
        ContractResp::Found {
            address,
            current_code_id,
            ..
        } => (address.parse()?, current_code_id),
    };
    let gitrev = match tracker
        .query(msg::contracts::tracker::entry::QueryMsg::CodeById { code_id })
        .await?
    {
        CodeIdResp::Found { gitrev, .. } => gitrev,
        CodeIdResp::NotFound {} => None,
    };
    Ok((addr, gitrev))
}

async fn get_tokens_markets(
    cosmos: &Cosmos,
    factory: Address,
) -> Result<(Vec<Cw20>, HashMap<MarketId, Address>)> {
    let factory = cosmos.make_contract(factory);
    let mut tokens = vec![];
    let mut markets_map = HashMap::new();
    let mut start_after = None;
    loop {
        let MarketsResp { markets } = factory
            .query(msg::contracts::factory::entry::QueryMsg::Markets {
                start_after: start_after.take(),
                limit: None,
            })
            .await?;
        match markets.last() {
            Some(x) => start_after = Some(x.clone()),
            None => break Ok((tokens, markets_map)),
        }

        for market_id in markets {
            let denom = market_id.get_collateral().to_owned();
            let market_info: MarketInfoResponse = factory
                .query(msg::contracts::factory::entry::QueryMsg::MarketInfo {
                    market_id: market_id.clone(),
                })
                .await?;
            let market_addr = market_info.market_addr.into_string().parse()?;
            markets_map.insert(market_id, market_addr);
            let market = cosmos.make_contract(market_addr);
            let StatusResp { collateral, .. } = market
                .query(msg::contracts::market::entry::QueryMsg::Status {})
                .await?;
            match collateral {
                msg::token::Token::Cw20 {
                    addr,
                    decimal_places,
                } => tokens.push(Cw20 {
                    address: addr.as_str().parse()?,
                    denom,
                    decimals: decimal_places,
                }),
                msg::token::Token::Native { .. } => (),
            }
        }
    }
}
