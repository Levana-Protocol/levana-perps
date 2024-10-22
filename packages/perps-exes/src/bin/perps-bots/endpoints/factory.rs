use std::collections::HashMap;

use axum::{extract::State, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use cosmos::{Address, HasAddress};
use cosmwasm_std::Addr;
use perps_exes::PerpsNetwork;
use perpswap::storage::MarketId;

use crate::{
    app::factory::{FactoryInfo, FrontendInfoTestnet},
    config::BotConfigByType,
};

use super::RestApp;

#[derive(serde::Serialize)]
struct FactoryResp<'a> {
    #[serde(flatten)]
    factory_info: FactoryInfoJson<'a>,
    #[serde(flatten)]
    frontend_info_testnet: Option<&'a FrontendInfoTestnet>,

    network: PerpsNetwork,
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
    pub(crate) copy_trading_addresses: Vec<Addr>
}

impl<'a> From<&'a FactoryInfo> for FactoryInfoJson<'a> {
    fn from(
        FactoryInfo {
            factory,
            updated,
            is_static,
            markets,
            copy_trading_addresses,
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
            copy_trading_addresses: copy_trading_addresses.to_vec()
        }
    }
}

pub(crate) async fn factory(rest_app: State<RestApp>) -> impl IntoResponse {
    let app = rest_app.0.app;
    let factory_info = app.get_factory_info().await;
    match &app.config.by_type {
        BotConfigByType::Testnet { inner } => Json(FactoryResp {
            factory_info: factory_info.as_ref().into(),
            frontend_info_testnet: app.get_frontend_info_testnet().await.as_deref(),
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
