use std::sync::Arc;

use anyhow::{Context, Result};
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
use resvg::usvg::{TreeParsing, TreeTextToPath};

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
        } = &self;
        let cosmos = app.cosmos.get(chain).ok_or(Error::UnknownChainId)?;
        let res: Result<PositionsResp> = cosmos
            .make_contract(*market)
            .query(msg::contracts::market::entry::QueryMsg::Positions {
                position_ids: vec![*position],
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
            Some(pos) => Ok(PnlInfo::new(self, pos)),
            None => Err(
                if res.positions.is_empty() && res.pending_close.is_empty() {
                    Error::PositionNotFound
                } else {
                    Error::PositionStillOpen
                },
            ),
        }
    }
}

impl PnlInfo {
    fn html(self) -> Response {
        Html(self.render().unwrap()).into_response()
    }

    fn image(self) -> Response {
        match self.image_inner() {
            Ok(res) => res,
            Err(e) => {
                let mut res = format!("Error while rendering SVG: {e:?}").into_response();
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                res
            }
        }
    }

    fn image_inner(self) -> Result<Response> {
        // Generate the raw SVG text by rendering the template
        let svg = PnlSvg {
            owner: self.owner,
            pnl_usd: self.pnl_usd,
        }
        .render()
        .unwrap();

        // Convert the SVG into a usvg tree using default settings
        let mut tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())?;

        // Load up the fonts and convert text values
        let mut fontdb = resvg::usvg::fontdb::Database::new();
        fontdb.load_system_fonts();
        tree.convert_text(&fontdb);

        // Now that our usvg tree has text converted, convert into an resvg tree
        let rtree = resvg::Tree::from_usvg(&tree);

        // Generate a new pixmap to hold the rasterized image
        let pixmap_size = rtree.size.to_int_size();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
            .context("Could not generate new Pixmap")?;

        // Render the rasterized image from the resvg SVG tree into the pixmap
        rtree.render(resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());

        // Take the binary PNG output and return is as a response
        let png = pixmap.encode_png()?;
        let mut res = png.into_response();
        res.headers_mut()
            .append(CONTENT_TYPE, HeaderValue::from_static("image/png"));
        Ok(res)
    }
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Unknown chain ID")]
    UnknownChainId,
    #[error("Could not find contract")]
    CouldNotFindContract,
    #[error("Could not query contract")]
    CouldNotQueryContract,
    #[error("Specified position not found")]
    PositionNotFound,
    #[error("The position is still open")]
    PositionStillOpen,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut res = self.to_string().into_response();
        *res.status_mut() = match self {
            Error::UnknownChainId => StatusCode::BAD_REQUEST,
            Error::CouldNotFindContract => StatusCode::BAD_REQUEST,
            Error::CouldNotQueryContract => StatusCode::BAD_REQUEST,
            Error::PositionNotFound => StatusCode::BAD_REQUEST,
            Error::PositionStillOpen => StatusCode::BAD_REQUEST,
        };
        res
    }
}

#[derive(askama::Template)]
#[template(path = "pnl.html")]
struct PnlInfo {
    owner: String,
    pnl_usd: Signed<Usd>,
    image_url: String,
}

impl PnlInfo {
    fn new(params: Params, pos: ClosedPosition) -> Self {
        PnlInfo {
            owner: pos.owner.into_string(),
            pnl_usd: pos.pnl_usd,
            image_url: params.image_url(),
        }
    }
}

impl Params {
    fn image_url(&self) -> String {
        format!(
            "/pnl/{chain}/{market}/{position}/image",
            chain = self.chain,
            market = self.market,
            position = self.position
        )
    }
}

#[derive(askama::Template)]
#[template(path = "pnl.svg.xml")]
struct PnlSvg {
    owner: String,
    pnl_usd: Signed<Usd>,
}
