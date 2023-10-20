use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};

use super::RestApp;

pub(crate) async fn gases(rest_app: State<RestApp>, _headers: HeaderMap) -> impl IntoResponse {
    Json(&*rest_app.app.gases.read().await).into_response()
}
