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
use cosmwasm_std::{Decimal256, Uint256};
use msg::{
    contracts::market::{
        entry::{QueryMsg, StatusResp},
        position::{PositionId, PositionsResp},
    },
    prelude::{NonZero, PricePoint, Signed, SignedLeverageToNotional, UnsignedDecimal, Usd},
};
use reqwest::{
    header::{CACHE_CONTROL, CONTENT_TYPE},
    StatusCode,
};
use resvg::usvg::{TreeParsing, TreeTextToPath};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    app::App,
    db::models::{PositionInfoFromDb, PositionInfoToDb},
    types::{ChainId, ContractEnvironment, DirectionForDb, PnlType, TwoDecimalPoints},
};

use super::{ErrorPage, PnlCssRoute, PnlHtml, PnlImage, PnlUrl};

pub(super) async fn pnl_url(
    _: PnlUrl,
    app: State<Arc<App>>,
    Json(position_info): Json<PositionInfo>,
) -> Result<Json<Value>, Error> {
    let db = &app.db;
    let to_db = position_info.get_info_to_db(&app).await?;
    let url_id = db
        .insert_position_detail(to_db)
        .await
        .map_err(|e| Error::Database { msg: e.to_string() })?;
    let url = PnlHtml { pnl_id: url_id };
    Ok(Json(json!({ "url": url.to_uri().to_string() })))
}

impl PnlInfo {
    async fn load_from_database(app: &App, pnl_id: i64, host: &Host) -> Result<Self, Error> {
        let PositionInfoFromDb {
            market_id,
            environment,
            pnl,
            direction,
            entry_price,
            exit_price,
            leverage,
            chain,
        } = app
            .db
            .get_url_detail(pnl_id)
            .await
            .map_err(|e| Error::Database { msg: e.to_string() })?
            .ok_or(Error::InvalidPage)?;
        Ok(PnlInfo {
            pnl_display: pnl,
            host: host.hostname().to_owned(),
            image_url: PnlImage { pnl_id }.to_uri().to_string(),
            html_url: PnlHtml { pnl_id }.to_uri().to_string(),
            market_id,
            direction,
            entry_price,
            exit_price,
            leverage,
            amplitude_key: environment.amplitude_key(),
            chain: chain.to_string(),
        })
    }
}

pub(super) async fn pnl_html(
    PnlHtml { pnl_id }: PnlHtml,
    TypedHeader(host): TypedHeader<Host>,
    State(app): State<Arc<App>>,
) -> Result<Response, Error> {
    PnlInfo::load_from_database(&app, pnl_id, &host)
        .await
        .map(PnlInfo::html)
}

pub(super) async fn pnl_image(
    PnlImage { pnl_id }: PnlImage,
    TypedHeader(host): TypedHeader<Host>,
    State(app): State<Arc<App>>,
) -> Result<Response, Error> {
    PnlInfo::load_from_database(&app, pnl_id, &host)
        .await
        .map(PnlInfo::image)
}

pub(super) async fn css(_: PnlCssRoute) -> Css<&'static str> {
    Css(include_str!("../../../../static/pnl.css"))
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub(crate) struct PositionInfo {
    pub(crate) address: Address,
    pub(crate) chain: ChainId,
    pub(crate) position_id: PositionId,
    pub(crate) pnl_type: PnlType,
}

struct MarketContract(Contract);

impl MarketContract {
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

impl PositionInfo {
    async fn get_info_to_db(self, app: &App) -> Result<PositionInfoToDb, Error> {
        let PositionInfo {
            chain,
            address,
            position_id,
            pnl_type,
        } = &self;
        let cosmos = app.cosmos.get(chain).ok_or(Error::UnknownChainId)?;

        // TODO check the database first to see if we need to insert this at all.
        let label = match cosmos.contract_info(*address).await {
            Ok(info) => Cow::Owned(info.label),
            Err(_) => "unknown contract".into(),
        };
        let contract = MarketContract(cosmos.make_contract(*address));

        let status = contract
            .query::<StatusResp>(QueryMsg::Status { price: None }, QueryType::Status)
            .await?;

        let mut res = contract
            .query::<PositionsResp>(
                QueryMsg::Positions {
                    position_ids: vec![*position_id],
                    skip_calc_pending_fees: None,
                    fees: None,
                    price: None,
                },
                QueryType::Positions,
            )
            .await?;

        let pos = match res.closed.pop() {
            Some(pos) => pos,
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
    Status,
    EntryPrice,
    ExitPrice,
    Positions,
}

#[derive(Debug, Clone)]
pub(crate) struct ErrorDescription {
    pub(crate) msg: String,
}

#[derive(thiserror::Error, Clone, Debug)]
pub(crate) enum Error {
    #[error("Unknown chain ID")]
    UnknownChainId,
    #[error("Specified position not found")]
    PositionNotFound,
    #[error("The position is still open")]
    PositionStillOpen,
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
                Error::UnknownChainId => StatusCode::BAD_REQUEST,
                Error::PositionNotFound => StatusCode::BAD_REQUEST,
                Error::PositionStillOpen => StatusCode::BAD_REQUEST,
                Error::FailedToQueryContract { query_type, msg: _ } => match query_type {
                    QueryType::Status => StatusCode::BAD_REQUEST,
                    QueryType::EntryPrice => StatusCode::INTERNAL_SERVER_ERROR,
                    QueryType::ExitPrice => StatusCode::INTERNAL_SERVER_ERROR,
                    QueryType::Positions => StatusCode::INTERNAL_SERVER_ERROR,
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
#[template(path = "pnl.html")]
struct PnlInfo {
    amplitude_key: &'static str,
    pnl_display: String,
    host: String,
    chain: String,
    image_url: String,
    html_url: String,
    market_id: String,
    direction: DirectionForDb,
    entry_price: String,
    exit_price: String,
    leverage: String,
}

struct UsdDisplay(Signed<Usd>);

impl Display for UsdDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let number = self.0.into_number();
        if number.is_negative() {
            write!(f, "-")?;
        } else {
            write!(f, "+")?;
        }
        let raw = number.abs_unsigned();

        let bigger = raw.to_uint_floor() / Uint256::from_u128(1000);
        add_commas(f, bigger)?;
        let rest =
            raw - Decimal256::from_ratio(bigger, 1u32) * Decimal256::from_ratio(1000u32, 1u32);

        if raw > Decimal256::from_ratio(1000u32, 1u32) {
            if rest.is_zero() {
                write!(f, "000")?;
            } else if rest < Decimal256::from_ratio(10u32, 1u32) {
                write!(f, "00")?;
            } else if rest < Decimal256::from_ratio(100u32, 1u32) {
                write!(f, "0")?;
            }
        }

        write!(f, "{} USD", TwoDecimalPoints(rest.into_signed()))
    }
}

fn add_commas(f: &mut std::fmt::Formatter, x: Uint256) -> std::fmt::Result {
    if x.is_zero() {
        Ok(())
    } else if x < Uint256::from_u128(1000) {
        write!(f, "{x},")
    } else {
        let bigger = x / Uint256::from_u128(1000);
        add_commas(f, bigger)?;
        let rest = x - bigger;
        write!(f, "{rest:0>3},")
    }
}

#[cfg(test)]
mod tests {
    use crate::endpoints::pnl::{TwoDecimalPoints, UsdDisplay};

    #[test]
    fn two_decimal_points() {
        assert_eq!(TwoDecimalPoints("1.2".parse().unwrap()).to_string(), "1.20");
        assert_eq!(
            TwoDecimalPoints("1.234".parse().unwrap()).to_string(),
            "1.23"
        );
        assert_eq!(
            TwoDecimalPoints("1.235".parse().unwrap()).to_string(),
            "1.24"
        );
    }

    #[test]
    fn usd_display() {
        assert_eq!(UsdDisplay("1.2".parse().unwrap()).to_string(), "+1.20 USD");
        assert_eq!(
            UsdDisplay("1.2345".parse().unwrap()).to_string(),
            "+1.23 USD"
        );
        assert_eq!(
            UsdDisplay("1.2355".parse().unwrap()).to_string(),
            "+1.24 USD"
        );
        assert_eq!(
            UsdDisplay("54321.2355".parse().unwrap()).to_string(),
            "+54,321.24 USD"
        );
        assert_eq!(
            UsdDisplay("-54321.2355".parse().unwrap()).to_string(),
            "-54,321.24 USD"
        );
        assert_eq!(
            UsdDisplay("-50001.2355".parse().unwrap()).to_string(),
            "-50,001.24 USD"
        );
    }
}

#[derive(askama::Template)]
#[template(path = "pnl.svg.xml")]
struct PnlSvg<'a> {
    info: &'a PnlInfo,
}
