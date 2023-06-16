use axum::{
    http::HeaderValue,
    response::{IntoResponse, Response},
};
use axum_extra::response::Css;
use reqwest::{header::CONTENT_TYPE, StatusCode};

use super::{ErrorPage, HomeRoute, HealthRoute, BuildVersionRoute, ErrorCssRoute, Favicon, RobotRoute};

pub(crate) async fn homepage(_ : HomeRoute) -> &'static str {
    r#"Welcome intrepid reader!

Not sure what you thought you'd find, but you didn't find it.

Better luck next time."#
}

pub(crate) async fn healthz(_: HealthRoute) -> &'static str {
    "Yup, I'm alive"
}

pub(crate) async fn build_version(_: BuildVersionRoute) -> &'static str {
    perps_exes::build_version()
}

pub(crate) async fn favicon(_: Favicon) -> Response {
    let mut res = include_bytes!("../../../../static/favicon.ico").into_response();
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("image/x-icon"));
    res
}

pub(crate) async fn robots_txt(_: RobotRoute) -> Response {
    let mut res = include_str!("../../../../static/robots.txt").into_response();
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    res
}

pub(crate) async fn not_found() -> ErrorPage<&'static str> {
    ErrorPage {
        error: "Page not found",
        code: StatusCode::NOT_FOUND,
    }
}

pub(super) async fn error_css(_: ErrorCssRoute) -> Css<&'static str> {
    Css(include_str!("../../../../static/error.css"))
}
