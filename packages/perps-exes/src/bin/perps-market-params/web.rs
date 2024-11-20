use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
};
use axum_extra::routing::RouterExt;
use parking_lot::RwLock;
use perpswap::storage::MarketId;
use tokio::task::JoinSet;

use crate::{
    cli::{Opt, ServeOpt},
    market_param::{compute_coin_dnfs, load_historical_data, DnfRecord, MarketStatusResult},
    routes::{HealthRoute, HistoryRoute, HomeRoute},
};

pub(crate) async fn axum_main(serve_opt: ServeOpt, opt: Opt) -> Result<()> {
    main_inner(serve_opt, opt).await
}

#[derive(Clone)]
pub(crate) struct NotifyApp {
    pub(crate) market_params: Arc<RwLock<HashMap<MarketId, MarketStatusResult>>>,
    pub(crate) markets: Arc<RwLock<HashSet<MarketId>>>,
    pub(crate) data_dir: PathBuf,
}

impl NotifyApp {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        NotifyApp {
            market_params: Arc::new(RwLock::new(HashMap::new())),
            markets: Arc::new(RwLock::new(HashSet::new())),
            data_dir,
        }
    }
}

async fn main_inner(serve_opt: ServeOpt, opt: Opt) -> Result<()> {
    let state = Arc::new(NotifyApp::new(serve_opt.cmc_data_dir.clone()));

    let mut set = JoinSet::new();
    let dnf_state = state.clone();
    set.spawn(async { compute_coin_dnfs(dnf_state, serve_opt, opt).await });

    let router = axum::Router::new()
        .typed_get(index)
        .typed_get(healthz)
        .typed_get(history)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;

    set.spawn(async {
        axum::serve(listener, router)
            .await
            .context("Background axum task should never complete")
    });

    // We should never exit...
    let res = set.join_next().await;
    set.abort_all();
    Err(anyhow::anyhow!("Unexpected join_next completion: {res:?}"))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) struct NoQueryString {}

#[derive(Template, serde::Serialize)]
#[template(path = "market_params.html")]
struct IndexTemplate {
    market_params: HashMap<MarketId, MarketStatusResult>,
    markets: HashSet<MarketId>,
}

pub(crate) async fn index(
    _: HomeRoute,
    app: State<Arc<NotifyApp>>,
    _: Query<NoQueryString>,
) -> axum::response::Response {
    let market_params = app.market_params.read().clone();
    let markets = app.markets.read().clone();
    let index_page = IndexTemplate {
        market_params,
        markets,
    }
    .render();
    match index_page {
        Ok(page) => Html::from(page).into_response(),
        Err(err) => {
            let res = Html::from(format!("Failure during template conversion {err}"));
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, res).into_response()
        }
    }
}

#[derive(Template, serde::Serialize)]
#[template(path = "mpa_history.html")]
struct HistoryTemplate {
    historical_records: Vec<DnfRecord>,
    market_id: MarketId,
}

pub(crate) async fn history(
    HistoryRoute { market_id }: HistoryRoute,
    app: State<Arc<NotifyApp>>,
    _: Query<NoQueryString>,
) -> axum::response::Response {
    let historical_data = load_historical_data(&market_id, app.data_dir.clone());
    let historical_data = match historical_data {
        Ok(result) => result,
        Err(err) => {
            let res = Html::from(format!("Failure during loading historical data: {err}"));
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, res).into_response();
        }
    };

    let history_page = HistoryTemplate {
        historical_records: historical_data.data,
        market_id,
    }
    .render();

    match history_page {
        Ok(page) => Html::from(page).into_response(),
        Err(err) => {
            let res = Html::from(format!("Failure during template conversion {err}"));
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, res).into_response()
        }
    }
}

pub(crate) async fn healthz(_: HealthRoute, _: Query<NoQueryString>) -> &'static str {
    "healthy"
}
