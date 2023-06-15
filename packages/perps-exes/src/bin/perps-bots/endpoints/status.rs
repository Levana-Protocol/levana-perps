use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, response::IntoResponse};

use crate::app::App;

pub(crate) async fn all(app: State<Arc<App>>, headers: HeaderMap) -> impl IntoResponse {
    let accept = headers.get("accept");

    if accept.map_or(false, |value| value.as_bytes().starts_with(b"text/html")) {
        app.statuses.all_statuses_html(&app.0)
    } else if accept.map_or(false, |value| {
        value.as_bytes().starts_with(b"application/json")
    }) {
        app.statuses.all_statuses_json(&app.0)
    } else {
        app.statuses.all_statuses_text()
    }
}
