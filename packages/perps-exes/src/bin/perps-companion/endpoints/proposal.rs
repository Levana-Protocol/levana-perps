use std::{borrow::Cow, sync::Arc};

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    extract::State,
    http::HeaderValue,
    response::{Html, IntoResponse, Response},
};
use axum_extra::response::Css;
use axum_extra::routing::TypedPath;
use axum_extra::TypedHeader;
use cosmos::{Address, Contract};
use cosmwasm_std::Uint64;
use headers::Host;
use resvg::usvg::{fontdb::Database, TreeParsing, TreeTextToPath};
use serde::{Deserialize, Serialize};

use crate::{
    app::App,
    types::{ChainId, ContractEnvironment},
};

use super::{Error, ProposalCssRoute, ProposalHtml, ProposalImage, ProposalImageSvg};

#[derive(askama::Template)]
#[template(path = "proposal.html")]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub(crate) struct ProposalInfo {
    id: Uint64,
    title: String,
    image_url: String,
    html_url: String,
    host: String,
    chain: ChainId,
    amplitude_key: String,
    address: Address,
}

impl ProposalInfo {
    async fn load(
        app: &App,
        proposal_id: i64,
        chain_id: ChainId,
        address: Address,
        host: &Host,
    ) -> Result<Self, Error> {
        let id_u64 = u64::try_from(proposal_id).map_err(|_| Error::ProposalNotFound)?;

        let cosmos = app.cosmos.get(&chain_id).ok_or(Error::UnknownChainId)?;
        let contract = GovContract(cosmos.make_contract(address));
        let label = match contract.0.info().await {
            Ok(info) => Cow::Owned(info.label),
            Err(_) => "unknown contract".into(),
        };
        let environment = ContractEnvironment::from_market(chain_id, &label);

        let mut res = contract
            .query::<ProposalsResp>(
                QueryMsg::ProposalsById {
                    ids: vec![id_u64.into()],
                },
                QueryType::Proposals,
            )
            .await?;

        let proposal = match res.0.pop() {
            Some(proposal) => proposal.proposal,
            None => return Err(Error::ProposalNotFound),
        };

        Ok(ProposalInfo {
            id: id_u64.into(),
            title: proposal.title,
            image_url: ProposalImage {
                proposal_id,
                chain_id,
                address,
            }
            .to_uri()
            .to_string(),
            html_url: ProposalHtml {
                proposal_id,
                chain_id,
                address,
            }
            .to_uri()
            .to_string(),
            host: host.hostname().to_owned(),
            amplitude_key: environment.amplitude_key().to_string(),
            chain: chain_id,
            address: address,
        })
    }
}

pub(super) async fn proposal_html(
    ProposalHtml {
        proposal_id,
        chain_id,
        address,
    }: ProposalHtml,
    TypedHeader(host): TypedHeader<Host>,
    State(app): State<Arc<App>>,
) -> Result<Response, Error> {
    ProposalInfo::load(&app, proposal_id, chain_id, address, &host)
        .await
        .map(ProposalInfo::html)
}

pub(super) async fn proposal_image(
    ProposalImage {
        proposal_id,
        chain_id,
        address,
    }: ProposalImage,
    TypedHeader(host): TypedHeader<Host>,
    State(app): State<Arc<App>>,
) -> Result<Response, Error> {
    ProposalInfo::load(&app, proposal_id, chain_id, address, &host)
        .await
        .map(|info| info.image(&app.fontdb))
}

pub(super) async fn proposal_image_svg(
    ProposalImageSvg {
        proposal_id,
        chain_id,
        address,
    }: ProposalImageSvg,
    TypedHeader(host): TypedHeader<Host>,
    State(app): State<Arc<App>>,
) -> Result<Response, Error> {
    ProposalInfo::load(&app, proposal_id, chain_id, address, &host)
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
                let e = Error::FailedToQueryGovContract {
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

fn get_title_lines(proposal: &ProposalInfo, max_length: usize, max_lines: usize) -> Vec<String> {
    let title = format!("#{} {}", proposal.id, proposal.title);
    let words = title.split_ascii_whitespace();
    let mut line = "".to_string();
    let mut text_lines = vec![];

    for word in words {
        // If we would go over the width limit with the current word,
        // and we have at least one word in the current line,
        // we save this line and add a new one.
        if !line.is_empty() && ((line.len() + word.len()) >= max_length) {
            text_lines.push(line);
            line = "".to_string();

            if text_lines.len() >= max_lines {
                break;
            }
        }

        // If we're already over the width limit, we save this line and add a new one.
        if line.len() >= max_length {
            text_lines.push(line);
            line = "".to_string();

            if text_lines.len() >= max_lines {
                break;
            }
        }

        // If we're not over the limit, we add the current word to the line being built.
        if line.len() < max_length {
            // If we already have at least one word in the current line, we separate it with a space.
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }

    // If the last line we were building didn't reach the limit, then we have to save those leftovers as well.
    if !line.is_empty() {
        text_lines.push(line);
    }

    text_lines
}

static TITLE_MAX_WIDTH: usize = 30;
static TITLE_MAX_LINES: usize = 6;

impl ProposalInfo {
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
        let svg = ProposalSvg {
            title_lines: get_title_lines(&self, TITLE_MAX_WIDTH, TITLE_MAX_LINES),
        }
        .render()
        .unwrap();

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

    fn image_inner(self, fontsdb: &Database) -> Result<Response> {
        // Generate the raw SVG text by rendering the template
        let svg = ProposalSvg {
            title_lines: get_title_lines(&self, TITLE_MAX_WIDTH, TITLE_MAX_LINES),
        }
        .render()
        .unwrap();

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

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ProposalQueryResponse {
    pub title: String,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ProposalRecordQueryResponse {
    pub proposal: ProposalQueryResponse,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ProposalsResp(Vec<ProposalRecordQueryResponse>);

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QueryMsg {
    ProposalsById { ids: Vec<Uint64> },
}

#[derive(askama::Template)]
#[template(path = "proposal.svg.xml")]
struct ProposalSvg {
    title_lines: Vec<String>,
}
