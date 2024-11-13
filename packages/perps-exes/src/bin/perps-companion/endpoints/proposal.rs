use std::{borrow::Cow, str::FromStr, sync::Arc};

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    extract::State,
    http::HeaderValue,
    response::{Html, IntoResponse, Response},
    Json,
};
use axum_extra::response::Css;
use axum_extra::routing::TypedPath;
use axum_extra::TypedHeader;
use cosmos::{error::AddressError, Address, Contract};
use cosmwasm_std::Uint64;
use headers::Host;
use resvg::usvg::{fontdb::Database, TreeParsing, TreeTextToPath};
use serde::Deserialize;
use serde_json::{json, Value};

use cosmwasm_schema::{cw_serde, QueryResponses};

use crate::{
    app::App,
    db::models::{ProposalInfoFromDb, ProposalInfoToDb},
    types::{ChainId, ContractEnvironment},
};

use super::{
    ErrorDescription, ErrorPage, ProposalCssRoute, ProposalHtml, ProposalImage, ProposalImageSvg,
    ProposalUrl,
};

#[derive(askama::Template)]
#[template(path = "proposal.html")]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub(crate) struct ProposalInfo {
    proposal_id: Uint64,
    title: String,
    image_url: String,
    html_url: String,
    host: String,
    chain: ChainId,
    amplitude_key: String,
    address: Address,
}

pub(super) async fn proposal_url(
    _: ProposalUrl,
    app: State<Arc<App>>,
    Json(proposal_info): Json<ProposalInfo>,
) -> Result<Json<Value>, Error> {
    let db = &app.db;
    let to_db = proposal_info.get_info_to_db(&app).await?;
    let url_id = db
        .insert_proposal_detail(to_db)
        .await
        .map_err(|e| Error::Database { msg: e.to_string() })?;
    let url = ProposalHtml {
        proposal_id: url_id,
    };
    Ok(Json(json!({ "url": url.to_uri().to_string() })))
}

impl ProposalInfo {
    async fn load_from_database(app: &App, proposal_id: u64, host: &Host) -> Result<Self, Error> {
        let ProposalInfoFromDb {
            title,
            environment,
            chain,
            address,
        }: ProposalInfoFromDb = app
            .db
            .get_proposal_detail(proposal_id)
            .await
            .map_err(|e| Error::Database { msg: e.to_string() })?
            .ok_or(Error::InvalidPage)?;
        Ok(ProposalInfo {
            proposal_id: proposal_id.into(),
            title: title,
            image_url: ProposalImage { proposal_id }.to_uri().to_string(),
            html_url: ProposalHtml { proposal_id }.to_uri().to_string(),
            host: host.hostname().to_owned(),
            amplitude_key: environment.amplitude_key().to_string(),
            chain,
            address: Address::from_str(&address)
                .map_err(|source| Error::InvalidAddress { source })?,
        })
    }
}

pub(super) async fn proposal_html(
    ProposalHtml { proposal_id }: ProposalHtml,
    TypedHeader(host): TypedHeader<Host>,
    State(app): State<Arc<App>>,
) -> Result<Response, Error> {
    ProposalInfo::load_from_database(&app, proposal_id, &host)
        .await
        .map(ProposalInfo::html)
}

pub(super) async fn proposal_image(
    ProposalImage { proposal_id }: ProposalImage,
    TypedHeader(host): TypedHeader<Host>,
    State(app): State<Arc<App>>,
) -> Result<Response, Error> {
    ProposalInfo::load_from_database(&app, proposal_id, &host)
        .await
        .map(|info| info.image(&app.fontdb))
}

pub(super) async fn proposal_image_svg(
    ProposalImageSvg { proposal_id }: ProposalImageSvg,
    TypedHeader(host): TypedHeader<Host>,
    State(app): State<Arc<App>>,
) -> Result<Response, Error> {
    ProposalInfo::load_from_database(&app, proposal_id, &host)
        .await
        .map(ProposalInfo::image_svg)
}

pub(super) async fn proposal_css(_: ProposalCssRoute) -> Css<&'static str> {
    Css(include_str!("../../../../static/proposal.css"))
}

struct GovContract(Contract);

impl GovContract {
    async fn query<T>(&self, msg: QueryMsg, query_type: QueryType) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut attempt = 1;
        loop {
            let res = self.0.query(&msg).await.map_err(|source| {
                let e = Error::FailedToQueryContract {
                    msg: msg.clone(),
                    query_type,
                };
                tracing::log::error!("Attempt #{attempt}: {e}. {source:?}");
                e
            });
            match res {
                Ok(x) => break Ok(x),
                Err(e) => {
                    if attempt >= 5 {
                        break Err(e);
                    } else {
                        attempt += 1;
                    }
                }
            }
        }
    }
}

impl ProposalInfo {
    async fn get_info_to_db(self, app: &App) -> Result<ProposalInfoToDb, Error> {
        let ProposalInfo {
            proposal_id,
            chain,
            address,
            ..
        } = &self;
        let cosmos = app.cosmos.get(chain).ok_or(Error::UnknownChainId)?;
        let label = match cosmos.make_contract(*address).info().await {
            Ok(info) => Cow::Owned(info.label),
            Err(_) => "unknown contract".into(),
        };
        let contract = GovContract(cosmos.make_contract(*address));

        let mut res = contract
            .query::<ProposalsResp>(
                QueryMsg::ProposalsById {
                    ids: vec![*proposal_id],
                },
                QueryType::Proposals,
            )
            .await?;

        match res.proposals.pop() {
            Some(proposal) => proposal,
            None => return Err(Error::ProposalNotFound),
        };

        Ok(ProposalInfoToDb {
            environment: ContractEnvironment::from_market(*chain, &label),
            proposal_id: self.proposal_id,
            title: self.title,
            chain: self.chain,
            address: self.address,
        })
    }

    fn html(self) -> Response {
        let mut res = Html(self.render().unwrap()).into_response();
        res.headers_mut().insert(
            http::header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=300"),
        );
        res
    }

    fn image(self, fontsdb: &Database) -> Response {
        match self.image_inner(fontsdb) {
            Ok(res) => res,
            Err(e) => {
                let mut res = format!("Error while rendering SVG: {e:?}").into_response();
                *res.status_mut() = http::status::StatusCode::INTERNAL_SERVER_ERROR;
                res
            }
        }
    }

    fn image_svg(self) -> Response {
        // Generate the raw SVG text by rendering the template
        let svg = ProposalSvg { info: &self }.render().unwrap();

        let mut res = svg.into_response();
        res.headers_mut().insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("image/svg+xml"),
        );
        res.headers_mut().insert(
            http::header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400"),
        );
        res
    }

    fn image_inner(&self, fontsdb: &Database) -> Result<Response> {
        // Generate the raw SVG text by rendering the template
        let svg = ProposalSvg { info: self }.render().unwrap();

        // Convert the SVG into a usvg tree using default settings
        let mut tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())?;

        tree.convert_text(fontsdb);

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
        res.headers_mut().insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("image/png"),
        );
        res.headers_mut().insert(
            http::header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400"),
        );
        Ok(res)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum QueryType {
    Proposals,
}

#[cw_serde]
pub struct ProposalQueryResponse {
    pub title: String,
}

#[cw_serde]
pub struct ProposalRecordQueryResponse {
    pub proposal: ProposalQueryResponse,
}

#[cw_serde]
pub struct ProposalsResp {
    pub proposals: Vec<ProposalRecordQueryResponse>,
}

#[cw_serde]
#[derive(QueryResponses)]
pub(crate) enum QueryMsg {
    #[returns(ProposalsResp)]
    ProposalsById { ids: Vec<Uint64> },
}

#[derive(thiserror::Error, Clone, Debug)]
pub(crate) enum Error {
    #[error("Unknown chain ID")]
    UnknownChainId,
    #[error("Specified proposal not found")]
    ProposalNotFound,
    #[error("Failed to query contract with {query_type:?}\nQuery: {msg:?}")]
    FailedToQueryContract {
        msg: QueryMsg,
        query_type: QueryType,
    },
    #[error("Error parsing path: {msg}")]
    Path { msg: String },
    #[error("Error returned from database")]
    Database { msg: String },
    #[error("Page not found")]
    InvalidPage,
    #[error("Invalid address: {source}")]
    InvalidAddress { source: AddressError },
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut response = ErrorPage {
            code: match &self {
                Error::ProposalNotFound => http::status::StatusCode::BAD_REQUEST,
                Error::FailedToQueryContract { query_type, msg: _ } => match query_type {
                    QueryType::Proposals => http::status::StatusCode::INTERNAL_SERVER_ERROR,
                },
                Error::Path { msg: _ } => http::status::StatusCode::BAD_REQUEST,
                Error::Database { msg } => {
                    tracing::error!("Database serror: {msg}");
                    http::status::StatusCode::INTERNAL_SERVER_ERROR
                }
                Error::InvalidPage => http::status::StatusCode::NOT_FOUND,
                Error::UnknownChainId => http::status::StatusCode::BAD_REQUEST,
                Error::InvalidAddress { source: _ } => http::status::StatusCode::BAD_REQUEST,
            },
            error: self.clone(),
        }
        .into_response();
        let error_description = ErrorDescription {
            msg: self.to_string(),
        };
        response.extensions_mut().insert(error_description);
        response
    }
}

#[derive(askama::Template)]
#[template(path = "proposal.svg.xml")]
struct ProposalSvg<'a> {
    info: &'a ProposalInfo,
}
