use axum::{
    http::HeaderValue,
    response::{IntoResponse, Response},
};
use reqwest::header::CONTENT_TYPE;

pub(crate) async fn homepage() -> &'static str {
    r#"Welcome intrepid reader!
    
Not sure what you thought you'd find, but you didn't find it.

Better luck next time."#
}

pub(crate) async fn healthz() -> &'static str {
    "Yup, I'm alive"
}

pub(crate) async fn build_version() -> &'static str {
    perps_exes::build_version()
}

pub(crate) async fn favicon() -> Response {
    let mut res = include_bytes!("../../../../static/favicon.ico").into_response();
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("image/x-icon"));
    res
}

pub(crate) async fn robots_txt() -> Response {
    let mut res = include_str!("../../../../static/robots.txt").into_response();
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    res
}
