use std::{collections::HashMap, sync::Arc};

use axum::{extract::State, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use cosmos::{Address, CosmosNetwork, HasAddress};
use shared::storage::MarketId;

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
    factory_info: FactoryInfoJson<'a>,
    #[serde(flatten)]
    frontend_info_testnet: Option<&'a FrontendInfoTestnet>,

    network: CosmosNetwork,
    price_api: &'a str,
    explorer: &'a str,
    maintenance: Option<&'a String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct FactoryInfoJson<'a> {
    pub(crate) factory: Address,
    pub(crate) updated: DateTime<Utc>,
    pub(crate) is_static: bool,
    pub(crate) markets: HashMap<&'a MarketId, Address>,
}

impl<'a> From<&'a FactoryInfo> for FactoryInfoJson<'a> {
    fn from(
        FactoryInfo {
            factory,
            updated,
            is_static,
            markets,
        }: &'a FactoryInfo,
    ) -> Self {
        FactoryInfoJson {
            factory: *factory,
            updated: *updated,
            is_static: *is_static,
            markets: markets
                .iter()
                .map(|market| (&market.market_id, market.market.get_address()))
                .collect(),
        }
    }
}

pub(crate) async fn factory(app: State<Arc<App>>) -> impl IntoResponse {
    let factory_info = app.get_factory_info();
    match &app.config.by_type {
        BotConfigByType::Testnet { inner } => Json(FactoryResp {
            factory_info: factory_info.as_ref().into(),
            frontend_info_testnet: app.get_frontend_info_testnet().as_deref(),
            network: app.config.network,
            price_api: &inner.price_api,
            explorer: &inner.explorer,
            maintenance: inner.maintenance.as_ref(),
        })
        .into_response(),
        BotConfigByType::Mainnet { .. } => {
            Json(FactoryInfoJson::from(&*factory_info)).into_response()
        }
    }
}
