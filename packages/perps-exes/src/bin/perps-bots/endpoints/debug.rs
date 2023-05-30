use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, response::IntoResponse};

use crate::app::App;

pub(crate) async fn gases(app: State<Arc<App>>, _headers: HeaderMap) -> impl IntoResponse {
    // 1000 records per address, I guess there is no need to make it pretty.
    let gases = app.gases.read();
    let response: String = format!("{gases:?}");
    response.into_response()
}
