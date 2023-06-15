use std::{fmt::Display, sync::Arc};

use anyhow::{Context, Result};
use askama::Template;
use axum::{
    extract::{Path, State},
    http::HeaderValue,
    response::{Html, IntoResponse, Response},
};
use cosmos::{Address, Contract};
use cosmwasm_std::{Decimal256, Uint256};
use msg::{
    contracts::market::{
        entry::{QueryMsg, StatusResp},
        position::{ClosedPosition, PositionId, PositionsResp},
    },
    prelude::{
        DirectionToBase, MarketId, NonZero, PriceBaseInQuote, PricePoint, Signed,
        SignedLeverageToNotional, UnsignedDecimal, Usd,
    },
};
use reqwest::{
    header::{CACHE_CONTROL, CONTENT_TYPE},
    StatusCode,
};
use resvg::usvg::{TreeParsing, TreeTextToPath};

use crate::app::App;

#[derive(serde::Deserialize, Debug)]
pub(super) struct Params {
    chain: String,
    market: Address,
    position: PositionId,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PnlType {
    Usd,
    Percent,
}

pub(super) async fn html_usd(app: State<Arc<App>>, params: Path<Params>) -> impl IntoResponse {
    params.0.with_pnl(&app, PnlInfo::html, PnlType::Usd).await
}

pub(super) async fn image_usd(app: State<Arc<App>>, params: Path<Params>) -> impl IntoResponse {
    params.0.with_pnl(&app, PnlInfo::image, PnlType::Usd).await
}

pub(super) async fn html_percent(app: State<Arc<App>>, params: Path<Params>) -> impl IntoResponse {
    params
        .0
        .with_pnl(&app, PnlInfo::html, PnlType::Percent)
        .await
}

pub(super) async fn image_percent(app: State<Arc<App>>, params: Path<Params>) -> impl IntoResponse {
    params
        .0
        .with_pnl(&app, PnlInfo::image, PnlType::Percent)
        .await
}

pub(super) async fn css() -> impl IntoResponse {
    let mut res = include_str!("../../../../static/pnl.css").into_response();
    res.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    res
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

impl Params {
    async fn with_pnl<F>(self, app: &App, f: F, pnl_type: PnlType) -> Response
    where
        F: FnOnce(PnlInfo) -> Response,
    {
        match self.get_pnl_info(app, pnl_type).await {
            Ok(pnl) => f(pnl),
            Err(e) => e.into_response(),
        }
    }

    async fn get_pnl_info(self, app: &App, pnl_type: PnlType) -> Result<PnlInfo, Error> {
        let Params {
            chain,
            market,
            position,
        } = &self;
        let cosmos = app.cosmos.get(chain).ok_or(Error::UnknownChainId)?;

        let contract = MarketContract(cosmos.make_contract(*market));

        let status = contract
            .query::<StatusResp>(QueryMsg::Status { price: None }, QueryType::Status)
            .await?;

        let mut res = contract
            .query::<PositionsResp>(
                QueryMsg::Positions {
                    position_ids: vec![*position],
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

        Ok(PnlInfo::new(
            self,
            pos,
            status.market_id,
            entry_price,
            exit_price,
            pnl_type,
        ))
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
enum QueryType {
    Status,
    EntryPrice,
    ExitPrice,
    Positions,
}

#[derive(thiserror::Error, Debug)]
enum Error {
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
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut res = self.to_string().into_response();
        *res.status_mut() = match self {
            Error::UnknownChainId => StatusCode::BAD_REQUEST,
            Error::PositionNotFound => StatusCode::BAD_REQUEST,
            Error::PositionStillOpen => StatusCode::BAD_REQUEST,
            Error::FailedToQueryContract { query_type, msg: _ } => match query_type {
                QueryType::Status => StatusCode::BAD_REQUEST,
                QueryType::EntryPrice => StatusCode::INTERNAL_SERVER_ERROR,
                QueryType::ExitPrice => StatusCode::INTERNAL_SERVER_ERROR,
                QueryType::Positions => StatusCode::INTERNAL_SERVER_ERROR,
            },
        };
        res
    }
}

#[derive(askama::Template)]
#[template(path = "pnl.html")]
struct PnlInfo {
    pnl_display: String,
    image_url: String,
    market_id: String,
    direction: &'static str,
    entry_price: PriceBaseInQuote,
    exit_price: PriceBaseInQuote,
    leverage: TwoDecimalPoints,
}

impl PnlInfo {
    fn new(
        params: Params,
        pos: ClosedPosition,
        market_id: MarketId,
        entry_price: PricePoint,
        exit_price: PricePoint,
        pnl_type: PnlType,
    ) -> Self {
        let market_type = market_id.get_market_type();
        PnlInfo {
            pnl_display: match pnl_type {
                PnlType::Usd => UsdDisplay(pos.pnl_usd).to_string(),
                PnlType::Percent => match pos.deposit_collateral.try_into_positive_value() {
                    None => "Negative collateral".to_owned(),
                    Some(deposit) => {
                        // FIXME we need a deposit_usd to do this accurately
                        let deposit = entry_price.collateral_to_usd(deposit);
                        let percent = pos.pnl_usd.into_number() / deposit.into_number()
                            * Decimal256::from_ratio(100u32, 1u32).into_signed();
                        let plus = if percent.is_negative() { "" } else { "+" };
                        format!("{plus}{}%", TwoDecimalPoints(percent))
                    }
                },
            },
            image_url: params.image_url(pnl_type),
            market_id: market_id.to_string().replace("_", "/"),
            direction: match pos.direction_to_base {
                DirectionToBase::Long => "LONG",
                DirectionToBase::Short => "SHORT",
            },
            entry_price: pos.entry_price_base,
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
                    .into_base(market_type)
                    .split()
                    .1
                    .into_number(),
                ),
            },
        }
    }
}

struct TwoDecimalPoints(Signed<Decimal256>);

impl Display for TwoDecimalPoints {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let ten = Decimal256::from_ratio(10u32, 1u32);
        let half = Decimal256::from_ratio(1u32, 2u32);

        if self.0.is_negative() {
            write!(f, "-")?;
        }

        let whole = self.0.abs_unsigned().floor();
        let rem = self.0.abs_unsigned() - whole;
        let rem = rem * ten;
        let x = rem.floor();
        let rem = rem - x;
        let rem = rem * ten;
        let y = rem.floor();
        let rem = rem - y;
        let y = if rem >= half {
            y + Decimal256::one()
        } else {
            y
        };
        write!(f, "{}.{}{}", whole, x, y)
    }
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

impl Params {
    fn image_url(&self, pnl_type: PnlType) -> String {
        format!(
            "/{pnl_type}/{chain}/{market}/{position}/image.png",
            pnl_type = match pnl_type {
                PnlType::Usd => "pnl-usd",
                PnlType::Percent => "pnl-percent",
            },
            chain = self.chain,
            market = self.market,
            position = self.position
        )
    }
}

#[derive(askama::Template)]
#[template(path = "pnl.svg.xml")]
struct PnlSvg<'a> {
    info: &'a PnlInfo,
}
