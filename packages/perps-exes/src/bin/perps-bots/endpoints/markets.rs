use axum::{
    http::HeaderValue,
    response::{IntoResponse, Response},
};
use reqwest::{header::LOCATION, StatusCode};

pub(crate) async fn markets() -> Response {
    let mut res = "Redirecting".into_response();
    res.headers_mut()
        .append(LOCATION, HeaderValue::from_static("/status#stats"));
    *res.status_mut() = StatusCode::TEMPORARY_REDIRECT;
    res
}
