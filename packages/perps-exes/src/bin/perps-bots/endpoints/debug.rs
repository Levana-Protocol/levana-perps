use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};

use crate::app::App;

pub(crate) async fn gases(app: State<Arc<App>>, _headers: HeaderMap) -> impl IntoResponse {
    Json(&*app.gases.read()).into_response()
}
