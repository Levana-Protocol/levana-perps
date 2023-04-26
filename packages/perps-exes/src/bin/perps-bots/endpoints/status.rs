use axum::{extract::Path, response::IntoResponse, Extension};

use crate::app::{status_collector::StatusCategory, App};

pub(crate) async fn all(app: Extension<App>) -> impl IntoResponse {
    app.get_status_collector().all()
}

pub(crate) async fn single(
    app: Extension<App>,
    Path(category): Path<StatusCategory>,
) -> impl IntoResponse {
    app.get_status_collector().single(category)
}
