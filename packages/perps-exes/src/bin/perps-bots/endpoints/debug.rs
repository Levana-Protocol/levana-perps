use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue},
    response::IntoResponse,
};
use reqwest::header::CONTENT_TYPE;

use crate::app::App;

pub(crate) async fn gases(app: State<Arc<App>>, _headers: HeaderMap) -> impl IntoResponse {
    let mut res = serde_json::to_string_pretty(&*app.gases.read())
        .expect("Error serializing JSON")
        .into_response();
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application.json"));
    res
}
