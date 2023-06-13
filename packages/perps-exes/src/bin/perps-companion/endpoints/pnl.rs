use std::sync::Arc;

use anyhow::Result;
use askama::Template;
use axum::{
    extract::{Path, State},
    http::HeaderValue,
    response::{Html, IntoResponse, Response},
};
use cosmos::Address;
use msg::{
    contracts::market::position::{ClosedPosition, PositionId, PositionsResp},
    prelude::{Signed, Usd},
};
use reqwest::{header::CONTENT_TYPE, StatusCode};

use crate::app::App;

#[derive(serde::Deserialize)]
pub(super) struct Params {
    chain: String,
    market: Address,
    position: PositionId,
}

pub(super) async fn html(app: State<Arc<App>>, params: Path<Params>) -> impl IntoResponse {
    params.0.with_pnl(&app, PnlInfo::html).await
}

pub(super) async fn image(app: State<Arc<App>>, params: Path<Params>) -> impl IntoResponse {
    params.0.with_pnl(&app, PnlInfo::image).await
}

pub(super) async fn css() -> impl IntoResponse {
    let mut res = include_str!("../../../../static/pnl.css").into_response();
    res.headers_mut().append(
        CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    res
}

impl Params {
    async fn with_pnl<F>(self, app: &App, f: F) -> Response
    where
        F: FnOnce(PnlInfo) -> Response,
    {
        match self.get_pnl_info(app).await {
            Ok(pnl) => f(pnl),
            Err(e) => e.into_response(),
        }
    }

    async fn get_pnl_info(self, app: &App) -> Result<PnlInfo, Error> {
        let Params {
            chain,
            market,
            position,
        } = self;
        let cosmos = app.cosmos.get(&chain).ok_or(Error::UnknownChainId)?;
        let res: Result<PositionsResp> = cosmos
            .make_contract(market)
            .query(msg::contracts::market::entry::QueryMsg::Positions {
                position_ids: vec![position],
                skip_calc_pending_fees: None,
                fees: None,
                price: None,
            })
            .await;
        let mut res = match res {
            Ok(res) => res,
            Err(_) => match cosmos.contract_info(market).await {
                Ok(_) => return Err(Error::CouldNotQueryContract),
                Err(_) => return Err(Error::CouldNotFindContract),
            },
        };

        match res.closed.pop() {
            Some(pos) => Ok(pos.into()),
            None => Err(Error::ClosedPositionNotFound),
        }
    }
}

impl PnlInfo {
    fn html(self) -> Response {
        Html(self.render().unwrap()).into_response()
    }

    fn image(self) -> Response {
        "FIXME image rendering not ready".into_response()
    }
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Unknown chain ID")]
    UnknownChainId,
    #[error("COuld not find contract")]
    CouldNotFindContract,
    #[error("Could not query contract")]
    CouldNotQueryContract,
    #[error("Specified closed position not found")]
    ClosedPositionNotFound,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut res = self.to_string().into_response();
        *res.status_mut() = match self {
            Error::UnknownChainId => StatusCode::BAD_REQUEST,
            Error::CouldNotFindContract => StatusCode::BAD_REQUEST,
            Error::CouldNotQueryContract => StatusCode::BAD_REQUEST,
            Error::ClosedPositionNotFound => StatusCode::BAD_REQUEST,
        };
        res
    }
}

#[derive(askama::Template)]
#[template(path = "pnl.html")]
struct PnlInfo {
    owner: String,
    pnl_usd: Signed<Usd>,
}

impl From<ClosedPosition> for PnlInfo {
    fn from(pos: ClosedPosition) -> Self {
        PnlInfo {
            owner: pos.owner.into_string(),
            pnl_usd: pos.pnl_usd,
        }
    }
}
