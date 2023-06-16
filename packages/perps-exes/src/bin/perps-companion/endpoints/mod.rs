mod common;
mod pnl;

use std::sync::Arc;

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    response::{Html, IntoResponse, Response},
    routing::get,
};
use reqwest::{header::CONTENT_TYPE, Method, StatusCode};
use tower_http::cors::CorsLayer;

use crate::app::App;

pub(crate) async fn launch(app: App) -> Result<()> {
    let bind = app.opt.bind;
    let app = Arc::new(app);
    let router = axum::Router::new()
        .route("/", get(common::homepage))
        .route("/healthz", get(common::healthz))
        .route("/build-version", get(common::build_version))
        .route("/pnl.css", get(pnl::css))
        .route("/error.css", get(common::error_css))
        .route("/pnl-usd/:chain/:market/:position", get(pnl::html_usd))
        .route(
            "/pnl-usd/:chain/:market/:position/image.png",
            get(pnl::image_usd),
        )
        .route(
            "/pnl-percent/:chain/:market/:position",
            get(pnl::html_percent),
        )
        .route(
            "/pnl-percent/:chain/:market/:position/image.png",
            get(pnl::image_percent),
        )
        .route("/favicon.ico", get(common::favicon))
        .route("/robots.txt", get(common::robots_txt))
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
