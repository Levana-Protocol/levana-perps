use std::sync::Arc;

use anyhow::Result;
use axum::routing::{get, post};

use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, timeout::TimeoutLayer};

use crate::{app::App, watcher::TaskStatuses};

pub(crate) mod carry;
pub(crate) mod common;
pub(crate) mod debug;
pub(crate) mod factory;
pub(crate) mod faucet;
pub(crate) mod markets;
pub(crate) mod status;

#[derive(Clone)]
pub(crate) struct RestApp {
    pub(crate) app: Arc<App>,
    pub(crate) statuses: TaskStatuses,
}

pub(crate) async fn start_rest_api(
    app: Arc<App>,
    statuses: TaskStatuses,
    listener: TcpListener,
) -> Result<()> {
    let service_builder = ServiceBuilder::new()
        .layer(RequestBodyLimitLayer::new(app.opt.request_body_limit_bytes))
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(
            app.opt.request_timeout_seconds,
        )))
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods([
                    http::method::Method::GET,
                    http::method::Method::HEAD,
                    http::method::Method::POST,
                ])
                .allow_headers([http::header::CONTENT_TYPE]),
        );

    let router = axum::Router::new()
        .route("/", get(common::homepage))
        .route("/factory", get(factory::factory))
        .route("/frontend-config", get(factory::factory))
        .route("/healthz", get(common::healthz))
        .route("/build-version", get(common::build_version))
        .route("/api/faucet", post(faucet::bot))
        .route("/status", get(status::all))
        .route("/carry", get(carry::carry))
        .route("/status/:label", get(status::single))
        .route("/markets", get(markets::markets))
        .route("/debug/gas-refill", get(debug::gas_refill))
        .route("/debug/fund-usage", get(debug::fund_usage))
        .with_state(RestApp { app, statuses })
        .layer(service_builder);

    tracing::info!("Launching server");

    axum::serve(listener, router.into_make_service()).await?;
    Err(anyhow::anyhow!("Background task should never complete"))
}
