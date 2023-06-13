mod common;
mod pnl;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::routing::get;
use reqwest::{header::CONTENT_TYPE, Method};
use tower_http::cors::CorsLayer;

use crate::app::App;

pub(crate) async fn launch(app: App) -> Result<()> {
    let bind = app.opt.bind;
    let app = Arc::new(app);
    let router = axum::Router::new()
        .route("/", get(common::homepage))
        .route("/healthz", get(common::healthz))
        .route("/build-version", get(common::build_version))
        .route("/pnl/:chain/:market/:position", get(pnl::html))
        .route("/pnl.css", get(pnl::css))
        .route("/pnl/:chain/:market/:position/image", get(pnl::image))
        .with_state(app)
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods([Method::GET, Method::HEAD, Method::POST])
                .allow_headers([CONTENT_TYPE]),
        );
    log::info!("Launching server");
    axum::Server::bind(&bind)
        .serve(router.into_make_service())
        .await
        .context("Background task should never complete")
}
