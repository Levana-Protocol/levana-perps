use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, Json};
use cosmos::CosmosNetwork;

use crate::{
    app::{
        factory::{FactoryInfo, FrontendInfoTestnet},
        App,
    },
    config::BotConfigByType,
};

#[derive(serde::Serialize)]
struct FactoryResp<'a> {
    #[serde(flatten)]
    factory_info: &'a FactoryInfo,
    #[serde(flatten)]
    frontend_info_testnet: Option<&'a FrontendInfoTestnet>,

    network: CosmosNetwork,
    price_api: &'a str,
    explorer: &'a str,
    maintenance: Option<&'a String>,
}

pub(crate) async fn factory(app: State<Arc<App>>) -> impl IntoResponse {
    let factory_info = app.get_factory_info();
    match &app.config.by_type {
        BotConfigByType::Testnet { inner } => Json(FactoryResp {
            factory_info: &factory_info,
            frontend_info_testnet: app.get_frontend_info_testnet().as_deref(),
            network: app.config.network,
            price_api: &inner.price_api,
            explorer: &inner.explorer,
            maintenance: inner.maintenance.as_ref(),
        })
        .into_response(),
        BotConfigByType::Mainnet { .. } => Json(&*factory_info).into_response(),
    }
}
