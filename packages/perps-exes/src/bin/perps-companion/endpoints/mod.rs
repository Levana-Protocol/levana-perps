mod common;
mod pnl;

use std::sync::Arc;

use anyhow::{Context, Result};
use askama::Template;
use axum::response::{Html, IntoResponse, Response};
use axum_extra::routing::{RouterExt, TypedPath};
use cosmos::Address;
use msg::contracts::market::position::PositionId;
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

#[derive(TypedPath, Deserialize)]
#[typed_path("/pnl-usd/:chain/:market/:position")]
pub(crate) struct PnlUsdHtml {
    chain: String,
    market: Address,
    position: PositionId,
}

#[derive(TypedPath, Deserialize)]
#[typed_path("/pnl-usd/:chain/:market/:position/image.png")]
pub(crate) struct PnlUsdImage {
    pub(crate) chain: String,
    pub(crate) market: Address,
    pub(crate) position: PositionId,
}

#[derive(TypedPath, Deserialize)]
#[typed_path("/pnl-percent/:chain/:market/:position")]
pub(crate) struct PnlPercentHtml {
    chain: String,
    market: Address,
    position: PositionId,
}

#[derive(TypedPath, Deserialize)]
#[typed_path("/pnl-percent/:chain/:market/:position/image.png")]
pub(crate) struct PnlPercentImage {
    pub(crate) chain: String,
    pub(crate) market: Address,
    pub(crate) position: PositionId,
}

#[derive(TypedPath)]
#[typed_path("/favicon.ico")]
pub(crate) struct Favicon;

#[derive(TypedPath)]
#[typed_path("/robots.txt")]
pub(crate) struct RobotRoute;

pub(crate) async fn launch(app: App) -> Result<()> {
    let bind = app.opt.bind;
    let app = Arc::new(app);
    let router = axum::Router::new()
        .typed_get(common::homepage)
        .typed_get(common::healthz)
        .typed_get(common::build_version)
        .typed_get(pnl::css)
        .typed_get(common::error_css)
        .typed_get(pnl::html_usd)
        .typed_get(pnl::image_usd)
        .typed_get(pnl::html_percent)
        .typed_get(pnl::image_percent)
        .typed_get(common::favicon)
        .typed_get(common::robots_txt)
        .fallback(common::not_found)
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
