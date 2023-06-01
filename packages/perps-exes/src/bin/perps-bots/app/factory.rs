use std::collections::HashMap;
use std::sync::Arc;

use axum::async_trait;
use chrono::{DateTime, Utc};
use cosmos::{Address, Cosmos};
use msg::contracts::faucet::entry::{GasAllowanceResp, TapAmountResponse};
use msg::prelude::*;
use msg::{
    contracts::{
        factory::entry::{MarketInfoResponse, MarketsResp},
        tracker::entry::{CodeIdResp, ContractResp},
    },
    token::Token,
};

use crate::config::BotConfigByType;
use crate::watcher::{Heartbeat, WatchedTask, WatchedTaskOutput};

use super::{App, AppBuilder};

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct FactoryInfo {
    pub(crate) factory: Address,
    pub(crate) updated: DateTime<Utc>,
    pub(crate) is_static: bool,
    pub(crate) markets: HashMap<MarketId, Address>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct FrontendInfoTestnet {
    pub(crate) faucet: Address,
    pub(crate) cw20s: Vec<Cw20>,
    pub(crate) gitrev: Option<String>,
    pub(crate) faucet_gas_amount: Option<String>,
    pub(crate) faucet_collateral_amount: HashMap<&'static str, Decimal256>,
    pub(crate) rpc: RpcInfo,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct RpcInfo {
    pub(crate) endpoint: String,
    pub(crate) rpc_height: u64,
    pub(crate) grpc_height: u64,
    pub(crate) latest_height: u64,
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct Cw20 {
    address: Address,
    denom: String,
    decimals: u8,
}

impl AppBuilder {
    pub(super) fn start_factory_task(&mut self) -> Result<()> {
        self.watch_periodic(crate::watcher::TaskLabel::GetFactory, FactoryUpdate)
    }
}

#[derive(Clone)]
struct FactoryUpdate;

#[async_trait]
impl WatchedTask for FactoryUpdate {
    async fn run_single(&mut self, app: &App, _heartbeat: Heartbeat) -> Result<WatchedTaskOutput> {
        update(app).await
    }
}

async fn update(app: &App) -> Result<WatchedTaskOutput> {
    let (message, info) = match &app.config.by_type {
        BotConfigByType::Testnet { inner } => {
            let (message, factory_info, frontend_info_testnet) = get_factory_info_testnet(
                &app.cosmos,
                &app.client,
                inner.tracker,
                inner.faucet,
                &inner.contract_family,
                &inner.rpc_nodes,
            )
            .await?;
            app.set_frontend_info_testnet(frontend_info_testnet)?;
            (message, factory_info)
        }
        BotConfigByType::Mainnet { inner } => {
            get_factory_info_mainnet(&app.cosmos, inner.factory).await?
        }
    };
    let output = WatchedTaskOutput {
        skip_delay: false,
        message,
    };
    app.set_factory_info(info);
    Ok(output)
}

pub(crate) async fn get_factory_info_mainnet(
    cosmos: &Cosmos,
    factory: Address,
) -> Result<(String, FactoryInfo)> {
    let message = format!("Using hard-coded factory address {factory}");

    let (_cw20s, markets) = get_tokens_markets(cosmos, factory)
        .await
        .with_context(|| format!("Unable to get_tokens_market for factory {factory}"))?;

    let factory_info = FactoryInfo {
        factory,
        updated: Utc::now(),
        is_static: false,
        markets,
    };
    Ok((message, factory_info))
}

pub(crate) async fn get_factory_info_testnet(
    cosmos: &Cosmos,
    client: &reqwest::Client,
    tracker: Address,
    faucet: Address,
    family: &str,
    rpc_nodes: &[Arc<String>],
) -> Result<(String, FactoryInfo, FrontendInfoTestnet)> {
    let (factory, gitrev) = get_contract(cosmos, tracker, family, "factory")
        .await
        .context("Unable to get 'factory' contract")?;
    let message = format!("Successfully loaded factory address {factory} from tracker {tracker}",);

    let (cw20s, markets) = get_tokens_markets(cosmos, factory)
        .await
        .with_context(|| format!("Unable to get_tokens_market for factory {factory}"))?;
    let faucet_gas_amount = match get_faucet_gas_amount(cosmos, faucet).await {
        Ok(x) => x,
        Err(e) => {
            log::warn!("Error on get_faucet_gas_amount: {e:?}");
            None
        }
    };

    let faucet_collateral_amount = match get_faucet_collateral_amount(cosmos, faucet).await {
        Ok(x) => x,
        Err(e) => {
            log::warn!("Error on get_faucet_collateral_amount: {e:?}");
            HashMap::new()
        }
    };

    let rpc = get_rpc_info(cosmos, client, rpc_nodes).await?;

    let factory_info = FactoryInfo {
        factory,
        updated: Utc::now(),
        is_static: false,
        markets,
    };
    let frontend_info_testnet = FrontendInfoTestnet {
        faucet,
        cw20s,
        gitrev,
        faucet_gas_amount,
        faucet_collateral_amount,
        rpc,
    };
    Ok((message, factory_info, frontend_info_testnet))
}

pub(crate) async fn get_contract(
    cosmos: &Cosmos,
    tracker: Address,
    family: &str,
    contract_type: &str,
) -> Result<(Address, Option<String>)> {
    let tracker = cosmos.make_contract(tracker);
    let (addr, code_id) = match tracker
        .query(msg::contracts::tracker::entry::QueryMsg::ContractByFamily {
            contract_type: contract_type.to_owned(),
            family: family.to_owned(),
            sequence: None,
        })
        .await
        .with_context(|| {
            format!("Calling ContractByFamily with {contract_type} and {family} against {tracker}",)
        })? {
        ContractResp::NotFound {} => {
            anyhow::bail!("No {contract_type} contract found for contract family {family}",)
        }
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

async fn get_faucet_gas_amount(cosmos: &Cosmos, faucet: Address) -> Result<Option<String>> {
    let contract = cosmos.make_contract(faucet);
    Ok(
        match contract
            .query(msg::contracts::faucet::entry::QueryMsg::GetGasAllowance {})
            .await?
        {
            GasAllowanceResp::Disabled {} => None,
            GasAllowanceResp::Enabled { denom: _, amount } => {
                // Somewhat hacky, but leverage the existing code on LpToken
                Some(LpToken::from_u128(amount.u128())?.to_string())
            }
        },
    )
}
async fn get_faucet_collateral_amount(
    cosmos: &Cosmos,
    faucet: Address,
) -> Result<HashMap<&'static str, Decimal256>> {
    let mut res = HashMap::new();
    let contract = cosmos.make_contract(faucet);
    for name in ["ATOM", "ETH", "BTC", "USDC"] {
        match contract
            .query(msg::contracts::faucet::entry::QueryMsg::TapAmountByName {
                name: name.to_owned(),
            })
            .await?
        {
            TapAmountResponse::CanTap { amount } => {
                res.insert(name, amount);
            }
            TapAmountResponse::CannotTap {} => (),
        }
    }
    Ok(res)
}

async fn get_rpc_info(
    cosmos: &Cosmos,
    client: &reqwest::Client,
    rpc_nodes: &[Arc<String>],
) -> Result<RpcInfo> {
    let grpc = cosmos.get_latest_block_info().await?;

    let mut handles = vec![];
    for node in rpc_nodes {
        handles.push(tokio::task::spawn(get_height(node.clone(), client.clone())));
    }

    let mut results = vec![];
    for handle in handles {
        match handle.await {
            Ok(Ok(pair)) => results.push(pair),
            Ok(Err(e)) => log::warn!("{e:?}"),
            Err(e) => log::warn!("{e:?}"),
        }
    }

    results.sort_by_key(|x| x.1);
    let (endpoint, rpc_height) = match results.into_iter().rev().next() {
        Some(pair) => pair,
        // All nodes are broken
        None => {
            let node = rpc_nodes.first().context("Config includes no RPC nodes")?;
            (node.clone(), 0)
        }
    };

    let grpc_height = grpc.height.try_into()?;

    Ok(RpcInfo {
        endpoint: (*endpoint).clone(),
        rpc_height,
        grpc_height,
        latest_height: rpc_height.max(grpc_height),
    })
}

async fn get_height(node: Arc<String>, client: reqwest::Client) -> Result<(Arc<String>, u64)> {
    let node_clone = node.clone();
    tokio::time::timeout(tokio::time::Duration::from_secs(3), async {
        let url = if node.ends_with('/') {
            format!("{node}status")
        } else {
            format!("{node}/status")
        };
        let value = client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        let height = get_latest_block_height(value)
            .context("Could not find latest block height in JSON response")?;
        anyhow::Ok((node, height))
    })
    .await
    .context("Timed out")
    .and_then(|x| x)
    .with_context(|| format!("Error getting height from {node_clone}"))
}

fn get_latest_block_height(value: serde_json::Value) -> Option<u64> {
    let mut values = vec![value];

    while let Some(value) = values.pop() {
        match value {
            serde_json::Value::Null => (),
            serde_json::Value::Bool(_) => (),
            serde_json::Value::Number(_) => (),
            serde_json::Value::String(_) => (),
            serde_json::Value::Array(mut xs) => values.append(&mut xs),
            serde_json::Value::Object(o) => {
                for (key, value) in o.into_iter() {
                    if key == "latest_block_height" {
                        if let serde_json::Value::String(x) = &value {
                            if let Ok(x) = x.parse() {
                                return Some(x);
                            }
                        }
                    }
                    values.push(value);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_latest_height() {
        const CONTENT: &str = r##"{"node_info":{"protocol_version":{"p2p":"8","block":"11","app":"0"},"id":"73204da3017b5d4e3756bde40274f55582936c69","listen_addr":"tcp://0.0.0.0:26656","network":"atlantic-2","version":"0.35.0-unreleased","channels":"40202122233038606162630070717273","moniker":"sei-rpc-i-050ef1199438a66dd","other":{"tx_index":"on","rpc_address":"tcp://0.0.0.0:26657"}},"application_info":{"version":"0"},"sync_info":{"latest_block_hash":"04BC42640308B4DEEF82C6E60CF8D7BFBC81EF7CABC56C0D1889D094DE0B470C","latest_app_hash":"632940CAFB3D21EEEFA347A263A22054A41D316AD2A61D89736ADBA16633991B","latest_block_height":"10998863","latest_block_time":"2023-05-21T08:10:46.345466442Z","earliest_block_hash":"24A7ECEE6B8BDE9A251676ACDBCAB7732C6704EFE1BF053449CE3BDB356A1FFA","earliest_app_hash":"93A7AB57325A7D465AC0A96CC84C3031210C47511623AB09CCF5CF8B7A704288","earliest_block_height":"10763999","earliest_block_time":"2023-05-19T19:53:34.042614538Z","max_peer_block_height":"10998858","catching_up":false,"total_synced_time":"0","remaining_time":"0","total_snapshots":"0","chunk_process_avg_time":"0","snapshot_height":"0","snapshot_chunks_count":"0","snapshot_chunks_total":"0","backfilled_blocks":"0","backfill_blocks_total":"0"},"validator_info":{"address":"BDF7022B1D6ED6BA3F93A79D4B97789F0B683161","pub_key":{"type":"tendermint/PubKeyEd25519","value":"h5xev2hlkao0xiNc2Xx/za5/ETDEbV7YxGamO7CDRBI="},"voting_power":"0"}}"##;
        let value: serde_json::Value = serde_json::from_str(CONTENT).unwrap();
        assert_eq!(get_latest_block_height(value), Some(10998863));
    }
}
