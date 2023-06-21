mod common;
mod export;
mod(crate)) pnl;
mod shared;

use std::sync::Arc;

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    extract::rejection::PathRejection,
    response::{Html, IntoResponse, Response},
};
use axum_extra::routing::{RouterExt, TypedPath};
use reqwest::{header::CONTENT_TYPE, Method, StatusCode};
use serde::Deserialize;
use tower_http::cors::CorsLayer;

use crate::app::App;

#[derive(TypedPath)]
#[typed_path("/")]
pub(crate) struct HomeRoute;

#[derive(TypedPath)]
#[typed_path("/healthz")]
pub(crate) struct HealthRoute;

#[derive(TypedPath)]
#[typed_path("/build-version")]
pub(crate) struct BuildVersionRoute;

#[derive(TypedPath)]
#[typed_path("/pnl.css")]
pub(crate) struct PnlCssRoute;

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

impl From<PathRejection> for pnl::Error {
    fn from(rejection: PathRejection) -> Self {
        Self::Path {
            msg: rejection.to_string(),
        }
    }
}

#[derive(TypedPath, Deserialize)]
#[typed_path("/export-history/:chain/:market/:wallet")]
pub(crate) struct ExportHistory {
    pub(crate) chain: String,
    pub(crate) market: Address,
    pub(crate) wallet: Address,
}

pub(crate) async fn launch(app: App) -> Result<()> {
    let bind = app.opt.bind;
    let app = Arc::new(app);
    let router = axum::Router::new()
        .typed_get(common::homepage)
        .typed_get(common::healthz)
        .typed_get(common::build_version)
        .typed_get(pnl::css)
        .typed_get(common::error_css)
        .typed_get(common::favicon)
        .typed_get(common::robots_txt)
        .typed_post(pnl::pnl_url)
        .typed_get(pnl::pnl_html)
        .typed_get(pnl::pnl_image)
        .typed_get(export::history)
        .with_state(app)
        .fallback(common::not_found)
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
