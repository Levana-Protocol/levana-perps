use std::sync::Arc;

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;

use crate::{
    app::App,
    watcher::{TaskLabel, TaskStatuses},
};

use super::RestApp;

pub(crate) async fn all(
    State(RestApp { app, statuses }): State<RestApp>,
    headers: HeaderMap,
) -> Response {
    helper(app, statuses, headers, None).await
}

pub(crate) async fn single(
    State(RestApp { app, statuses }): State<RestApp>,
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
    helper(app, statuses, headers, Some(label)).await
}

async fn helper(
    app: Arc<App>,
    statuses: TaskStatuses,
    headers: HeaderMap,
    label: Option<TaskLabel>,
) -> Response {
    let accept = headers.get("accept");

    if accept.map_or(false, |value| value.as_bytes().starts_with(b"text/html")) {
        statuses.statuses_html(&app, label).await
    } else if accept.map_or(false, |value| {
        value.as_bytes().starts_with(b"application/json")
    }) {
        statuses.statuses_json(&app, label).await
    } else {
        statuses.statuses_text(label).await
    }
}
