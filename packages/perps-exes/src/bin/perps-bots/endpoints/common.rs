use axum::response::{IntoResponse, Response};
use http::{header::LOCATION, HeaderValue, StatusCode};

pub(crate) async fn homepage() -> Response {
    let mut res = r#"Welcome intrepid reader!
    
Not sure what you thought you'd find, but you didn't find it.

Better luck next time."#
        .into_response();
    *res.status_mut() = StatusCode::FOUND;
    res.headers_mut()
        .insert(LOCATION, HeaderValue::from_static("/status"));
    res
}

pub(crate) async fn healthz() -> &'static str {
    "Yup, I'm alive"
}

pub(crate) async fn build_version() -> &'static str {
    perps_exes::build_version()
}
