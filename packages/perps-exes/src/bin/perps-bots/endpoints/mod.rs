use axum::routing::{get, post};
use reqwest::{header::CONTENT_TYPE, Method};
use tower_http::cors::CorsLayer;

use crate::app::AppBuilder;

pub(crate) mod carry;
pub(crate) mod common;
pub(crate) mod debug;
pub(crate) mod factory;
pub(crate) mod faucet;
pub(crate) mod markets;
pub(crate) mod status;

impl AppBuilder {
    pub(crate) fn start_rest_api(
        &mut self,
        server: hyper::server::Builder<hyper::server::conn::AddrIncoming>,
    ) {
        let app = self.app.clone();

        self.watch_background(async move {
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
                .route("/debug/gas-usage", get(debug::gases))
                .with_state(app)
                .layer(
                    CorsLayer::new()
                        .allow_origin(tower_http::cors::Any)
                        .allow_methods([Method::GET, Method::HEAD, Method::POST])
                        .allow_headers([CONTENT_TYPE]),
                );
            log::info!("Launching server");

            server.serve(router.into_make_service()).await?;
            Err(anyhow::anyhow!("Background task should never complete"))
        });
    }
}
