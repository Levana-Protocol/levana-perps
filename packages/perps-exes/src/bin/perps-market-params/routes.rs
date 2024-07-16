use axum_extra::routing::TypedPath;
use serde::Deserialize;
use shared::storage::MarketId;

#[derive(TypedPath)]
#[typed_path("/")]
pub(crate) struct HomeRoute;

#[derive(TypedPath)]
#[typed_path("/healthz")]
pub(crate) struct HealthRoute;

#[derive(TypedPath, Deserialize)]
#[typed_path("/historical/:market_id")]
pub(crate) struct HistoryRoute {
    pub(crate) market_id: MarketId,
}
