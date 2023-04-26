use axum::{response::IntoResponse, Extension, Json};

use crate::app::{factory::FactoryInfo, App, FrontendInfo};

#[derive(serde::Serialize)]
struct FactoryResp<'a> {
    #[serde(flatten)]
    factory_info: &'a FactoryInfo,
    #[serde(flatten)]
    frontend_info: &'a FrontendInfo,
}

pub(crate) async fn factory(app: Extension<App>) -> impl IntoResponse {
    let factory_info = app.get_factory_info();
    Json(FactoryResp {
        factory_info: &factory_info,
        frontend_info: &app.frontend_info,
    })
    .into_response()
}
