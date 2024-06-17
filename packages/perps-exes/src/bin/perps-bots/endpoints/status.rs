use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
};

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
            *res.status_mut() = http::status::StatusCode::BAD_REQUEST;
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
    let user_agent = headers.get("user-agent");

    if is_uptime_robot(user_agent).unwrap_or_default() {
        statuses.statuses_text(&app, label).await
    } else if accept.map_or(false, |value| value.as_bytes().starts_with(b"text/html")) {
        statuses.statuses_html(&app, label).await
    } else if accept.map_or(false, |value| {
        value.as_bytes().starts_with(b"application/json")
    }) {
        statuses.statuses_json(&app, label).await
    } else {
        statuses.statuses_text(&app, label).await
    }
}

fn is_uptime_robot(user_agent: Option<&HeaderValue>) -> Option<bool> {
    let user_agent = user_agent?;
    let user_agent = std::str::from_utf8(user_agent.as_bytes()).ok()?;
    Some(user_agent.contains("UptimeRobot"))
}
