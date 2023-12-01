use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};

use super::RestApp;

pub(crate) async fn gases(rest_app: State<RestApp>, _headers: HeaderMap) -> impl IntoResponse {
    Json(&*rest_app.app.gas_refill.read().await).into_response()
}

pub(crate) async fn price_wallet(rest_app: State<RestApp>, _headers: HeaderMap) -> impl IntoResponse {
    Json(&*rest_app.app.gas_usage.read().await).into_response()
}
