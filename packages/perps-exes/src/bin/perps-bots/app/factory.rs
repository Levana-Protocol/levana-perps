use std::collections::HashMap;

use axum::async_trait;
use chrono::{DateTime, Utc};
use cosmos::{Address, Cosmos};
use msg::prelude::*;
use msg::{
    contracts::{
        factory::entry::{MarketInfoResponse, MarketsResp},
        tracker::entry::{CodeIdResp, ContractResp},
    },
    token::Token,
};

use crate::config::BotConfig;
use crate::watcher::{Heartbeat, WatchedTask, WatchedTaskOutput};

use super::{App, AppBuilder};

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

impl AppBuilder {
    pub(super) fn launch_factory_task(&mut self) -> Result<()> {
        self.watch_periodic(crate::watcher::TaskLabel::GetFactory, FactoryUpdate)
    }
}

#[derive(Clone)]
struct FactoryUpdate;

#[async_trait]
impl WatchedTask for FactoryUpdate {
    async fn run_single(&self, app: &App, _heartbeat: Heartbeat) -> Result<WatchedTaskOutput> {
        update(app).await
    }
}

async fn update(app: &App) -> Result<WatchedTaskOutput> {
    let info = get_factory_info(&app.cosmos, &app.config).await?;
    let output = WatchedTaskOutput {
        skip_delay: false,
        message: format!(
            "Successfully loaded factory address {} from tracker {}",
            info.factory, app.config.tracker
        ),
    };
    app.set_factory_info(info);
    Ok(output)
}

pub(crate) async fn get_factory_info(cosmos: &Cosmos, config: &BotConfig) -> Result<FactoryInfo> {
    let (factory, gitrev) = get_contract(cosmos, config, "factory")
        .await
        .context("Unable to get 'factory' contract")?;
    let (cw20s, markets) = get_tokens_markets(cosmos, factory)
        .await
        .with_context(|| format!("Unable to get_tokens_market for factory {factory}"))?;
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

pub(crate) async fn get_contract(
    cosmos: &Cosmos,
    config: &BotConfig,
    contract_type: &str,
) -> Result<(Address, Option<String>)> {
    let tracker = cosmos.make_contract(config.tracker);
    let (addr, code_id) = match tracker
        .query(msg::contracts::tracker::entry::QueryMsg::ContractByFamily {
            contract_type: contract_type.to_owned(),
            family: config.contract_family.clone(),
            sequence: None,
        })
        .await
        .with_context(|| {
            format!(
                "Calling ContractByFamily with {contract_type} and {} against {tracker}",
                config.contract_family
            )
        })? {
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

            // Simplify backwards compatibility issues: only look at the field we care about
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct StatusRespJustCollateral {
                collateral: Token,
            }
            let StatusRespJustCollateral { collateral } = market
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
