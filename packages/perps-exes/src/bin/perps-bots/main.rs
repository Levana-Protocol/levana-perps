use anyhow::Result;
use app::App;
use axum::{
    routing::{get, post},
    Extension,
};
use clap::Parser;
use reqwest::{header::CONTENT_TYPE, Method};
use tower_http::cors::CorsLayer;

mod app;
mod cli;
mod endpoints;
mod market_contract;
mod util;

#[tokio::main]
async fn main() -> Result<()> {
    main_inner().await
}

async fn main_inner() -> Result<()> {
    dotenv::dotenv().ok();

    let opt = cli::Opt::parse();
    opt.init_logger();
    let bind = opt.bind;
    let app = App::load(opt).await?;

    let router = axum::Router::new()
        .route("/", get(endpoints::common::homepage))
        .route("/factory", get(endpoints::factory::factory))
        .route("/frontend-config", get(endpoints::factory::factory))
        .route("/healthz", get(endpoints::common::healthz))
        .route("/build-version", get(endpoints::common::build_version))
        .route("/api/faucet", post(endpoints::faucet::bot))
        .route("/status", get(endpoints::status::all))
        .route("/status/:category", get(endpoints::status::single))
        .route("/epochs", get(endpoints::epochs::show_epochs))
        .route("/markets", get(endpoints::markets::markets))
        .layer(Extension(app))
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

    Ok(())
}
