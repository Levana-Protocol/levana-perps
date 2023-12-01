use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};

use super::RestApp;

pub(crate) async fn gas_refill(rest_app: State<RestApp>, _headers: HeaderMap) -> impl IntoResponse {
    Json(&*rest_app.app.gas_refill.read().await).into_response()
}

pub(crate) async fn gas_usage(rest_app: State<RestApp>, _headers: HeaderMap) -> impl IntoResponse {
    Json(&*rest_app.app.gas_usage.read().await).into_response()
}
