use std::{borrow::Cow, fmt::Display, sync::Arc};

use anyhow::{Context, Result};
use askama::Template;
use async_channel::{Receiver, Sender};
use axum::{
    extract::State,
    http::HeaderValue,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::{response::Css, routing::TypedPath};
use chrono::NaiveDate;
use cosmos::CosmosNetwork;
use cosmwasm_std::Decimal256;
use futures::StreamExt;
use hyper::{header::CONTENT_TYPE, HeaderMap, StatusCode};
use perps_exes::{
    contracts::{Factory, MarketInfo},
    prelude::MarketContract,
};
use reqwest::Client;
use shared::storage::{MarketId, Signed, UnsignedDecimal};
use tokio::task::JoinSet;

use crate::{app::App, types::ContractEnvironment};

use super::WhaleCssRoute;

pub(super) async fn whale_css(_: WhaleCssRoute) -> Css<&'static str> {
    Css(include_str!("../../../../static/whale.css"))
}

#[derive(TypedPath)]
#[typed_path("/whales")]
pub(crate) struct Whales;

pub(super) async fn whales(_: Whales, app: State<Arc<App>>, headers: HeaderMap) -> Response {
    match whales_inner(&app, &headers).await {
        Ok(res) => res,
        Err(e) => {
            log::error!("Error loading whales page: {e:?}");
            let mut res = "Internal error occurred".into_response();
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            res
        }
    }
}

async fn whales_inner(app: &App, headers: &HeaderMap) -> Result<Response> {
    let whale_data = load_whale_data(app).await?;

    let accept = headers.get("accept");

    if accept.map_or(false, |value| {
        value.as_bytes().starts_with(b"application/json")
    }) {
        Ok(Json(whale_data).into_response())
    } else {
        whale_data.to_html()
    }
}

#[derive(askama::Template)]
#[template(path = "whale.html")]
struct HtmlData<'a> {
    amplitude_key: &'a str,
    data: &'a WhaleData,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
struct WhaleData {
    markets: Vec<WhaleMarketData>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
struct WhaleMarketData {
    chain: SimpleCosmosNetwork,
    market_id: MarketId,
    long_funding: String,
    short_funding: String,
    lp_apr_1d: Cow<'static, str>,
    xlp_apr_1d: Cow<'static, str>,
    lp_apr_7d: Cow<'static, str>,
    xlp_apr_7d: Cow<'static, str>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum SimpleCosmosNetwork {
    Injective,
    Osmosis,
    Sei,
}

impl Display for SimpleCosmosNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            SimpleCosmosNetwork::Injective => "injective",
            SimpleCosmosNetwork::Osmosis => "osmosis",
            SimpleCosmosNetwork::Sei => "sei",
        })
    }
}

fn ratio_to_percent(r: Signed<Decimal256>) -> String {
    to_percent(&(r * Decimal256::from_ratio(100u8, 1u8).into_signed()).to_string())
}

fn to_percent(s: &str) -> String {
    format!("{}%", s.chars().take(7).collect::<String>())
}

#[derive(Debug)]
enum Work {
    Factory(CosmosNetwork, Factory, Sender<Work>),
    Market(CosmosNetwork, MarketInfo),
}

async fn load_whale_data(app: &App) -> Result<WhaleData> {
    let mut set = JoinSet::<Result<()>>::new();
    let (send_work, recv_work) = async_channel::unbounded::<Work>();
    let (send_market, recv_market) = async_channel::unbounded::<WhaleMarketData>();

    for _ in 0..8 {
        set.spawn(worker(
            recv_work.clone(),
            send_market.clone(),
            app.client.clone(),
        ));
    }

    for (factory, network) in &app.factories {
        send_work
            .send(Work::Factory(*network, factory.clone(), send_work.clone()))
            .await?;
    }

    std::mem::drop(send_work);
    std::mem::drop(send_market);

    while let Some(res) = set.join_next().await {
        match res {
            Err(e) => {
                set.abort_all();
                return Err(e.into());
            }
            Ok(Err(e)) => {
                set.abort_all();
                return Err(e);
            }
            Ok(Ok(())) => (),
        }
    }

    let mut markets = recv_market.collect::<Vec<_>>().await;
    markets.sort_by_cached_key(|x| (x.chain.to_string(), x.market_id.to_string()));
    Ok(WhaleData { markets })
}

async fn worker(
    recv_work: Receiver<Work>,
    send_market: Sender<WhaleMarketData>,
    client: reqwest::Client,
) -> Result<()> {
    while let Ok(work) = recv_work.recv().await {
        log::info!("Work: {work:?}");
        match work {
            Work::Factory(network, factory, send_work) => {
                let markets = factory.get_markets().await?;
                for market in markets {
                    send_work.send(Work::Market(network, market)).await?;
                }
            }
            Work::Market(network, market_info) => {
                let market_data = load_whale_market_data(network, market_info, &client).await?;
                send_market.send(market_data).await?;
            }
        }
    }
    Ok(())
}

/// Overall market status information
///
/// Returned from [QueryMsg::Status]
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct StatusRelaxed {
    pub long_funding: Signed<Decimal256>,
    pub short_funding: Signed<Decimal256>,
}

#[derive(serde::Deserialize)]
struct AprDailyAvg {
    lp: String,
    xlp: String,
    date: NaiveDate,
}

async fn load_whale_market_data(
    network: CosmosNetwork,
    market_info: MarketInfo,
    client: &reqwest::Client,
) -> Result<WhaleMarketData> {
    let market = MarketContract::new(market_info.market);
    let StatusRelaxed {
        long_funding,
        short_funding,
    } = market.status_relaxed().await?;

    let (lp_apr_1d, xlp_apr_1d) = get_aprs(
        client,
        &format!("https://indexer.levana.finance/apr_daily_avg?market={market}"),
    )
    .await?;
    let (lp_apr_7d, xlp_apr_7d) = get_aprs(
        client,
        &format!("https://indexer.levana.finance/apr?market={market}"),
    )
    .await?;

    Ok(WhaleMarketData {
        chain: match network {
            CosmosNetwork::OsmosisMainnet => SimpleCosmosNetwork::Osmosis,
            CosmosNetwork::SeiMainnet => SimpleCosmosNetwork::Sei,
            CosmosNetwork::InjectiveMainnet => SimpleCosmosNetwork::Injective,
            _ => anyhow::bail!("Unsupported network: {network}"),
        },
        market_id: match market_info.market_id.as_str() {
            "axlETH_USD" => "ETH_USD".parse()?,
            _ => market_info.market_id,
        },
        long_funding: ratio_to_percent(long_funding),
        short_funding: ratio_to_percent(short_funding),
        lp_apr_1d,
        xlp_apr_1d,
        lp_apr_7d,
        xlp_apr_7d,
    })
}

async fn get_aprs(client: &Client, url: &str) -> Result<(Cow<'static, str>, Cow<'static, str>)> {
    let aprs = async {
        client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<AprDailyAvg>>()
            .await
            .map_err(anyhow::Error::from)
    }
    .await
    .with_context(|| format!("Error while loading data from {url}"))?;
    Ok(match aprs.into_iter().max_by_key(|x| x.date) {
        Some(AprDailyAvg { lp, xlp, date: _ }) => (to_percent(&lp).into(), to_percent(&xlp).into()),
        None => ("-".into(), "-".into()),
    })
}

impl WhaleData {
    fn to_html(&self) -> Result<Response> {
        let s = HtmlData {
            amplitude_key: ContractEnvironment::Mainnet.amplitude_key(),
            data: self,
        }
        .render()?;
        let mut res = s.into_response();
        res.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        Ok(res)
    }
}
