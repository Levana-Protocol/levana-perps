mod common;
mod export;
pub(crate) mod pnl;
pub(crate) mod proposal;
mod whales;

use std::sync::Arc;

use anyhow::{Context, Result};
use askama::Template;
use axum::extract::Request;
use axum::{
    extract::rejection::PathRejection,
    middleware::{from_fn, Next},
    response::{Html, IntoResponse, Response},
    Json,
};
use axum_extra::routing::{RouterExt, TypedPath};
use cosmos::Address;
use http::status::StatusCode;

use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{self, TraceLayer};
use tracing::Level;

use crate::app::App;
use crate::types::ChainId;

use self::pnl::ErrorDescription;

#[derive(TypedPath)]
#[typed_path("/")]
pub(crate) struct HomeRoute;

#[derive(TypedPath)]
#[typed_path("/healthz")]
pub(crate) struct HealthRoute;

#[derive(TypedPath)]
#[typed_path("/grpc-health")]
pub(crate) struct GrpcHealthRoute;

#[derive(TypedPath)]
#[typed_path("/build-version")]
pub(crate) struct BuildVersionRoute;

#[derive(TypedPath)]
#[typed_path("/pnl.css")]
pub(crate) struct PnlCssRoute;

#[derive(TypedPath)]
#[typed_path("/proposal.css")]
pub(crate) struct ProposalCssRoute;

#[derive(TypedPath)]
#[typed_path("/whale.css")]
pub(crate) struct WhaleCssRoute;

#[derive(TypedPath)]
#[typed_path("/error.css")]
pub(crate) struct ErrorCssRoute;

#[derive(TypedPath)]
#[typed_path("/favicon.ico")]
pub(crate) struct Favicon;

#[derive(TypedPath)]
#[typed_path("/robots.txt")]
pub(crate) struct RobotRoute;

#[derive(TypedPath, Deserialize)]
#[typed_path("/pnl-url")]
pub(crate) struct PnlUrl;

#[derive(TypedPath, Deserialize)]
#[typed_path("/pnl/:pnl_id", rejection(pnl::Error))]
pub(crate) struct PnlHtml {
    pub(crate) pnl_id: i64,
}
#[derive(TypedPath, Deserialize)]
#[typed_path("/pnl/:pnl_id/image.png", rejection(pnl::Error))]
pub(crate) struct PnlImage {
    pub(crate) pnl_id: i64,
}

#[typed_path("/pnl/:pnl_id/image.svg", rejection(pnl::Error))]
pub(crate) struct PnlImageSvg {
    pub(crate) pnl_id: i64,
}

#[derive(TypedPath, Deserialize)]
#[typed_path("/proposal-url")]
pub(crate) struct ProposalUrl;

#[derive(TypedPath, Deserialize)]
#[typed_path("/proposal/:proposal_id", rejection(proposal::Error))]
pub(crate) struct ProposalHtml {
    pub(crate) proposal_id: u64,
}

#[derive(TypedPath, Deserialize)]
#[typed_path("/proposal/:proposal_id/image.png", rejection(proposal::Error))]
pub(crate) struct ProposalImage {
    pub(crate) proposal_id: u64,
}

impl From<PathRejection> for pnl::Error {
    fn from(rejection: PathRejection) -> Self {
        Self::Path {
            msg: rejection.to_string(),
        }
    }
}

impl From<PathRejection> for proposal::Error {
    fn from(rejection: PathRejection) -> Self {
        Self::Path {
            msg: rejection.to_string(),
        }
    }
}

#[derive(TypedPath, Deserialize)]
#[typed_path("/export-history/:chain/:factory/:wallet")]
pub(crate) struct ExportHistory {
    pub(crate) chain: ChainId,
    pub(crate) factory: Address,
    pub(crate) wallet: Address,
}

pub(crate) async fn launch(app: App) -> Result<()> {
    let bind = app.opt.bind;

    let app = Arc::new(app);

    let service_builder = ServiceBuilder::new()
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        )
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
                    http::method::Method::PUT,
                ])
                .allow_headers([http::header::CONTENT_TYPE]),
        );

    let router = axum::Router::new()
        .typed_get(common::homepage)
        .typed_get(common::healthz)
        .typed_get(common::grpc_health)
        .typed_get(common::build_version)
        .typed_get(pnl::pnl_css)
        .typed_get(common::error_css)
        .typed_get(common::favicon)
        .typed_get(common::robots_txt)
        .typed_post(pnl::pnl_url)
        .typed_put(pnl::pnl_url)
        .typed_get(pnl::pnl_html)
        .typed_get(pnl::pnl_image)
        .typed_get(pnl::pnl_image_svg)
        .typed_post(proposal::proposal_url)
        .typed_put(proposal::proposal_url)
        .typed_get(proposal::proposal_html)
        .typed_get(proposal::proposal_image)
        .typed_get(export::history)
        .typed_get(whales::whales)
        .typed_get(whales::whale_css)
        .with_state(app)
        .fallback(common::not_found)
        .layer(service_builder)
        .layer(from_fn(error_response_handler));

    tracing::info!("Launching server");
    let listener = TcpListener::bind(&bind).await?;
    axum::serve(listener, router.into_make_service())
        .await
        .context("Background task should never complete")
}

async fn error_response_handler(request: Request, next: Next) -> Response {
    let accept_header = request
        .headers()
        .get(&http::header::ACCEPT)
        .map(|value| value.as_ref().to_owned());

    let mut response = next.run(request).await;

    let status_code = response.status();

    if let Some(error_description) = response.extensions_mut().remove::<ErrorDescription>() {
        let msg = error_description.msg;
        match accept_header.as_deref() {
            Some(b"application/json") => return Json(json!({ "error": msg })).into_response(),
            Some(b"text/plain") => {
                let text_response = format!("error: {msg}");
                return (status_code, text_response).into_response();
            }
            _ => return response,
        }
    }
    response
}

#[derive(askama::Template)]
#[template(path = "error.html")]
pub(crate) struct ErrorPage<T: std::fmt::Display> {
    error: T,
    code: StatusCode,
}

impl<T: std::fmt::Display> IntoResponse for ErrorPage<T> {
    fn into_response(self) -> Response {
        let mut res = Html(self.render().unwrap()).into_response();
        *res.status_mut() = self.code;
        res
    }
}
