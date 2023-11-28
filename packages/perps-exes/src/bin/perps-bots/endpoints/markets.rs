use axum::{
    http::HeaderValue,
    response::{IntoResponse, Response},
};
use reqwest::{header::LOCATION, StatusCode};

pub(crate) async fn markets() -> Response {
    let mut res = "Redirecting".into_response();
    res.headers_mut().append(
        http::header::LOCATION,
        HeaderValue::from_static("/status#stats"),
    );
    *res.status_mut() = http::status::StatusCode::TEMPORARY_REDIRECT;
    res
}
