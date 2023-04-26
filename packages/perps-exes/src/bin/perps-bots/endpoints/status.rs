use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, response::IntoResponse};

use crate::app::App;

pub(crate) async fn all(app: State<Arc<App>>, headers: HeaderMap) -> impl IntoResponse {
    let wants_html = headers
        .get("accept")
        .map_or(false, |value| value.as_bytes().starts_with(b"text/html"));
    if wants_html {
        app.statuses.all_statuses_html()
    } else {
        app.statuses.all_statuses_text()
    }
}
