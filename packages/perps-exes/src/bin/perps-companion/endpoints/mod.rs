mod common;
pub(crate) mod pnl;

use std::sync::Arc;

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    extract::rejection::PathRejection,
    response::{Html, IntoResponse, Response}, http::Request, middleware::{Next, from_fn},
};
use axum_extra::routing::{RouterExt, TypedPath};
use reqwest::{header::{CONTENT_TYPE, ACCEPT}, Method, StatusCode};
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
        .with_state(app)
        .fallback(common::not_found)
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods([Method::GET, Method::HEAD, Method::POST])
                .allow_headers([CONTENT_TYPE]),
        )
        .layer(from_fn(error_response_handler));

    log::info!("Launching server");
    axum::Server::bind(&bind)
        .serve(router.into_make_service())
        .await
        .context("Background task should never complete")
}

async fn error_response_handler<B>(request: Request<B>, next: Next<B>) -> Response {
    let accept_header = request
        .headers()
        .get(&ACCEPT)
        .map(|value| value.as_ref().to_owned());

    let mut response = next.run(request).await;

    let status_code = response.status();

    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Status
    // if status_code

    if let Some(my_data) = response.extensions_mut().remove::<MyData>() {
        match accept_header.as_deref() {
            Some(b"application/json") => return Json(my_data).into_response(),
            Some(b"html") => return Html(format!("<body>{}</body>", my_data.data)).into_response(),
            _ => { /* yield original 501 response */ }
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
