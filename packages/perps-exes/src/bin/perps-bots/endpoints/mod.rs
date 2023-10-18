use std::net::SocketAddr;

use anyhow::{bail, Context, Result};
use axum::routing::{get, post};
use reqwest::{header::CONTENT_TYPE, Method};
use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::{info_span, Instrument, Level, instrument};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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
        server: SocketAddr,
    ) -> Result<()> {
        let app = self.app.clone();

        tracing::info!("sibi debug: {}", self.sentry_guard.is_enabled());



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
            )
            .into_make_service();

        self.watch_background(async move {
            // tracing_subscriber::registry()
            //     .with(
            //         fmt::Layer::default()
            //             .and_then(EnvFilter::from_default_env().add_directive(Level::INFO.into())),
            //     )
            //     .with(sentry_tracing::layer())
            //     .init();

	    let server = axum::Server::try_bind(&server)
                .with_context(|| format!("Cannot launch bot HTTP service bound to {}", server))?;


            server.serve(router).await?;
            Ok(())
        });
        Ok(())
    }
}
