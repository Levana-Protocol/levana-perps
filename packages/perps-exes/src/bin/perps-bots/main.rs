use anyhow::Result;
use app::AppBuilder;
use axum::routing::{get, post};
use clap::Parser;
use reqwest::{header::CONTENT_TYPE, Method};
use tower_http::cors::CorsLayer;

mod app;
mod cli;
pub(crate) mod config;
mod endpoints;
mod util;
pub(crate) mod watcher;

#[tokio::main]
async fn main() -> Result<()> {
    main_inner().await
}

async fn main_inner() -> Result<()> {
    dotenv::dotenv().ok();

    let opt = cli::Opt::parse();
    opt.init_logger();
    let mut app_builder = opt.into_app_builder().await?;
    app_builder.launch_rest_api();
    app_builder.load().await?;

    app_builder.wait().await
}

impl AppBuilder {
    fn launch_rest_api(&mut self) {
        let app = self.app.clone();

        self.watch_background(async move {
            let bind = app.bind;
            let router = axum::Router::new()
                .route("/", get(endpoints::common::homepage))
                .route("/factory", get(endpoints::factory::factory))
                .route("/frontend-config", get(endpoints::factory::factory))
                .route("/healthz", get(endpoints::common::healthz))
                .route("/build-version", get(endpoints::common::build_version))
                .route("/api/faucet", post(endpoints::faucet::bot))
                .route("/status", get(endpoints::status::all))
                .route("/markets", get(endpoints::markets::markets))
                .route("/debug/gses", get(endpoints::debug::gases))
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
                .await?;
            Err(anyhow::anyhow!("Background task should never complete"))
        });
    }
}
