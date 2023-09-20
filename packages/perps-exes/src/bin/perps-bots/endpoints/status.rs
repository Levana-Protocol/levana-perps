use std::sync::Arc;

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;

use crate::{app::App, watcher::TaskLabel};

pub(crate) async fn all(app: State<Arc<App>>, headers: HeaderMap) -> Response {
    helper(app, headers, None).await
}

pub(crate) async fn single(
    app: State<Arc<App>>,
    headers: HeaderMap,
    label: axum::extract::Path<String>,
) -> Response {
    let label = match TaskLabel::from_slug(&label) {
        Some(label) => label,
        None => {
            let mut res = "Invalid status label".into_response();
            *res.status_mut() = StatusCode::BAD_REQUEST;
            return res;
        }
    };
    helper(app, headers, Some(label)).await
}

async fn helper(app: State<Arc<App>>, headers: HeaderMap, label: Option<TaskLabel>) -> Response {
    let accept = headers.get("accept");

    if accept.map_or(false, |value| value.as_bytes().starts_with(b"text/html")) {
        app.statuses.statuses_html(&app.0, label).await
    } else if accept.map_or(false, |value| {
        value.as_bytes().starts_with(b"application/json")
    }) {
        app.statuses.statuses_json(&app.0, label).await
    } else {
        app.statuses.statuses_text(label).await
    }
}
