use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
};
use axum_extra::routing::RouterExt;
use parking_lot::RwLock;
use tokio::task::JoinSet;

use crate::{
    coingecko::Coin,
    market_param::compute_coin_dnfs,
    routes::{HealthRoute, HomeRoute},
};

#[tokio::main(flavor = "multi_thread")]
pub(crate) async fn axum_main(coins: Vec<Coin>) -> Result<()> {
    main_inner(coins).await
}

#[derive(Clone)]
pub(crate) struct NotifyApp {
    pub(crate) dnf: Arc<RwLock<HashMap<Coin, f64>>>,
}

impl NotifyApp {
    pub(crate) fn new() -> Self {
        NotifyApp {
            dnf: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

async fn main_inner(coins: Vec<Coin>) -> Result<()> {
    let state = Arc::new(NotifyApp::new());

    let mut set = JoinSet::new();
    let my_state = state.clone();
    let my_coins = coins.clone();
    set.spawn_blocking(|| {
        compute_coin_dnfs(my_state, my_coins)
    });

    let router = axum::Router::new()
        .typed_get(index)
        .typed_get(healthz)
        .with_state(state.clone());

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
    dnf: HashMap<Coin, f64>,
}

pub(crate) async fn index(
    _: HomeRoute,
    app: State<Arc<NotifyApp>>,
    _: Query<NoQueryString>,
) -> axum::response::Response {
    let dnf = app.dnf.read().clone();
    let index_page = IndexTemplate { dnf }.render();
    match index_page {
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
