use std::{borrow::Cow, fmt::Display, sync::Arc};

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    extract::State,
    headers::Host,
    http::HeaderValue,
    response::{Html, IntoResponse, Response},
    Json, TypedHeader,
};
use axum_extra::response::Css;
use axum_extra::routing::TypedPath;
use cosmos::{Address, Contract};
use cosmwasm_std::Uint64;
use reqwest::{
    header::{CACHE_CONTROL, CONTENT_TYPE},
    StatusCode,
};
use resvg::usvg::{TreeParsing, TreeTextToPath};
use serde::Deserialize;
use serde_json::{json, Value};

use cosmwasm_schema::{cw_serde, QueryResponses};

use crate::{
    app::App,
    db::models::{ProposalInfoFromDb, ProposalInfoToDb}, types::ChainId,
};

use super::{ErrorPage, ProposalCssRoute, ProposalHtml, ProposalImage, ProposalUrl};

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
    let url = ProposalHtml { proposal_id: url_id };
    Ok(Json(json!({ "url": url.to_uri().to_string() })))
}

impl ProposalInfo {
    async fn load_from_database(app: &App, proposal_id: u64, host: &Host) -> Result<Self, Error> {
        let ProposalInfoFromDb {
            title,
            environment,
            chain,
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
            amplitude_key: environment.amplitude_key(),
            chain: chain.to_string(),
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
        .map(ProposalInfo::image)
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
                log::error!("Attempt #{attempt}: {e}. {source:?}");
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

#[derive(askama::Template)]
#[template(path = "proposal.html")]
struct ProposalInfo {
    proposal_id: Uint64,
    title: String,
    image_url: String,
    html_url: String,
    host: String,
    chain: ChainId,
    amplitude_key: &'static str,
    address: Address,
}

impl ProposalInfo {
    async fn get_info_to_db(self, app: &App) -> Result<ProposalInfoToDb, Error> {
        let ProposalInfo {
            proposal_id,
            title,
            image_url,
            html_url,
            host,
            amplitude_key,
            chain,
            address,
        } = &self;
        let cosmos = app.cosmos.get(chain).ok_or(Error::UnknownChainId)?;
        let label = match cosmos.contract_info(*address).await {
            Ok(info) => Cow::Owned(info.label),
            Err(_) => "unknown contract".into(),
        };
        let contract = GovContract(cosmos.make_contract(*address));

        let mut res = contract
            .query::<ProposalsResp>(
                QueryMsg::ProposalsById {
                    ids: vec![*proposal_id],
                },
                QueryType::Positions,
            )
            .await?;

        let proposal = match res.closed.pop() {
            Some(proposal) => proposal,
            None => {
                return Err(
                    if res.positions.is_empty() && res.pending_close.is_empty() {
                        Error::PositionNotFound
                    } else {
                        Error::PositionStillOpen
                    },
                )
            }
        };

        let entry_price = contract
            .query::<PricePoint>(
                QueryMsg::SpotPrice {
                    timestamp: Some(pos.created_at),
                },
                QueryType::EntryPrice,
            )
            .await?;

        let exit_price: PricePoint = contract
            .query::<PricePoint>(
                QueryMsg::SpotPrice {
                    timestamp: Some(pos.settlement_time),
                },
                QueryType::ExitPrice,
            )
            .await?;

        let deposit_collateral_usd = if pos.deposit_collateral_usd.is_zero() {
            // Old data doesn't have this field, so it defaults to 0. We assume
            // that if we see 0, it's just a default value and we need to
            // hackily calculate this.
            pos.deposit_collateral
                .map(|x| entry_price.collateral_to_usd(x))
        } else {
            pos.deposit_collateral_usd
        };

        Ok(PositionInfoToDb {
            market_id: status.market_id,
            direction: pos.direction_to_base.into(),
            entry_price: entry_price.price_base,
            exit_price: exit_price.price_base,
            leverage: match NonZero::new(pos.active_collateral) {
                // Total liquidation occurred, which (1) should virtually never
                // happen and (2) wouldn't be a celebration. Just using 0.
                None => TwoDecimalPoints(Decimal256::zero().into_number()),
                Some(active_collateral) => TwoDecimalPoints(
                    SignedLeverageToNotional::calculate(
                        pos.notional_size,
                        &exit_price,
                        active_collateral,
                    )
                    .into_base(status.market_type)
                    .split()
                    .1
                    .into_number(),
                ),
            }
            .to_string(),
            environment: ContractEnvironment::from_market(*chain, &label),
            pnl: match pnl_type {
                PnlType::Usd => UsdDisplay(pos.pnl_usd).to_string(),
                PnlType::Percent => match deposit_collateral_usd.try_into_positive_value() {
                    None => "Negative collateral".to_owned(),
                    Some(deposit) => {
                        let percent = pos.pnl_usd.into_number() / deposit.into_number()
                            * Decimal256::from_ratio(100u32, 1u32).into_signed();
                        let plus = if percent.is_negative() { "" } else { "+" };
                        format!("{plus}{}%", TwoDecimalPoints(percent))
                    }
                },
            },
            info: self,
        })
    }

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

    fn image_inner(&self) -> Result<Response> {
        // Generate the raw SVG text by rendering the template
        let svg = PnlSvg { info: self }.render().unwrap();

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
            .insert(CONTENT_TYPE, HeaderValue::from_static("image/png"));
        res.headers_mut().insert(
            CACHE_CONTROL,
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
    ProposalsById {
        ids: Vec<Uint64>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ErrorDescription {
    pub(crate) msg: String,
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
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut response = ErrorPage {
            code: match &self {
                Error::ProposalNotFound => StatusCode::BAD_REQUEST,
                Error::FailedToQueryContract { query_type, msg: _ } => match query_type {
                    QueryType::Proposals => StatusCode::INTERNAL_SERVER_ERROR,
                },
                Error::Path { msg: _ } => StatusCode::BAD_REQUEST,
                Error::Database { msg } => {
                    log::error!("Database serror: {msg}");
                    StatusCode::INTERNAL_SERVER_ERROR
                }
                Error::InvalidPage => StatusCode::NOT_FOUND,
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
