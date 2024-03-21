use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
};
use axum_extra::routing::RouterExt;
use parking_lot::RwLock;

use crate::{
    coingecko::Coin,
    routes::{HealthRoute, HomeRoute},
};

#[tokio::main(flavor = "multi_thread")]
pub(crate) async fn axum_main() -> Result<()> {
    main_inner().await
}

#[derive(Clone)]
pub(crate) struct NotifyApp {
    dnf: Arc<RwLock<HashMap<Coin, f64>>>,
}

impl NotifyApp {
    pub(crate) fn new() -> Self {
        NotifyApp {
            dnf: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

async fn main_inner() -> Result<()> {
    let state = Arc::new(NotifyApp::new());

    let router = axum::Router::new()
        .typed_get(index)
        .typed_get(healthz)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, router).await?;
    Ok(())
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
